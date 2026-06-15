export default {
  async scheduled(event, env, ctx) {
    ctx.waitUntil(refreshDueSources(env));
  },

  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname;
    const hostname = url.hostname;

    // Route pds.dais.social to PDS worker
    if (hostname === 'pds.dais.social') {
      const targetUrl = env.PDS_URL + path + url.search;
      const targetRequest = new Request(targetUrl, {
        method: request.method,
        headers: request.headers,
        body: request.body,
      });
      return fetch(targetRequest);
    }

    if (hostname === 'social.dais.social' && path === '/') {
      return Response.redirect(new URL('/users/social', url).toString(), 302);
    }

    if (path === '/__dais-fixtures/activitypub/actor') {
      return fixtureActivityPubActor(url);
    }

    if (path.startsWith('/api/dais/owner/')) {
      return handleOwnerApi(request, env, url);
    }

    if (path.startsWith('/api/v1/') || path.startsWith('/api/v2/') || path.startsWith('/oauth/')) {
      return handleMastodonApi(request, env, url);
    }

    if (request.method === 'GET' && path === '/.well-known/oauth-authorization-server') {
      return oauthAuthorizationServerMetadata(url);
    }

    if (request.method === 'GET' && path === '/.well-known/openid-configuration') {
      return oauthAuthorizationServerMetadata(url);
    }

    if (request.method === 'GET' && path === '/.well-known/nodeinfo') {
      return nodeInfoDiscovery(url);
    }

    if (request.method === 'GET' && path === '/nodeinfo/2.0') {
      return nodeInfoDocument(env);
    }

    // Serve media files from R2
    if (path.startsWith('/media/')) {
      return handleMedia(request, env, path);
    }

    // Route to appropriate worker based on path
    let targetUrl;

    if (path.startsWith('/.well-known/webfinger')) {
      targetUrl = env.WEBFINGER_URL + path + url.search;
    } else if (path.match(/^\/@[^\/]+$/)) {
      const username = path.slice(2);
      targetUrl = env.ACTOR_URL + `/users/${encodeURIComponent(username)}` + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/inbox/)) {
      targetUrl = env.INBOX_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/outbox/)) {
      targetUrl = env.OUTBOX_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/posts\//)) {
      targetUrl = env.OUTBOX_URL + path + url.search;
    } else if (path.match(/^\/messages\/[^\/]+/)) {
      targetUrl = env.ACTOR_URL + path + url.search;
    } else if (path.startsWith('/admin/followers/')) {
      targetUrl = env.DELIVERY_QUEUE_URL + path.replace('/admin', '') + url.search;
    } else if (path.startsWith('/admin/deliveries/')) {
      targetUrl = env.DELIVERY_QUEUE_URL + path.replace('/admin', '') + url.search;
    } else if (path.match(/^\/users\/[^\/]+/)) {
      targetUrl = env.ACTOR_URL + path + url.search;
    } else {
      return new Response('Not Found', { status: 404 });
    }

    // Proxy the request to the target worker
    const targetRequest = new Request(targetUrl, {
      method: request.method,
      headers: request.headers,
      body: request.body,
    });

    return fetch(targetRequest);
  },
};

async function refreshDueSources(env) {
  const now = new Date().toISOString();
  const { results } = await env.DB.prepare(`
    SELECT id, source_type, url, refresh_cadence_minutes, etag, last_modified, policy_json, api_secret_name
    FROM source_subscriptions
    WHERE status = 'active'
      AND source_type IN ('rss', 'atom', 'api')
      AND (next_fetch_at IS NULL OR next_fetch_at <= ?)
    ORDER BY COALESCE(next_fetch_at, created_at) ASC
    LIMIT 20
  `).bind(now).all();

  for (const source of results || []) {
    try {
      await refreshFeedSource(env, source);
    } catch (error) {
      await env.DB.prepare(`
        UPDATE source_subscriptions
        SET status = 'error',
            last_error = ?,
            error_count = error_count + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
      `).bind(String(error && error.message ? error.message : error).slice(0, 500), source.id).run();
    }
  }
}

async function refreshFeedSource(env, source) {
  const headers = new Headers({ 'User-Agent': 'dais-source-refresh/1.0' });
  if (source.etag) headers.set('If-None-Match', source.etag);
  if (source.last_modified) headers.set('If-Modified-Since', source.last_modified);
  if (source.api_secret_name && env[source.api_secret_name]) {
    headers.set('Authorization', `Bearer ${env[source.api_secret_name]}`);
  }

  const response = await fetch(source.url, { headers });
  const nextFetchAt = new Date(
    Date.now() + Math.max(5, Number(source.refresh_cadence_minutes || 60)) * 60 * 1000
  ).toISOString();

  if (response.status === 304) {
    await markSourceRefreshed(env, source, nextFetchAt, source.etag, source.last_modified);
    return;
  }
  if (!response.ok) {
    throw new Error(`source fetch failed with HTTP ${response.status}`);
  }

  const policy = parsePolicy(source.policy_json);
  const body = await response.text();
  const items = (source.source_type === 'api'
    ? parseApiItems(body, source, policy)
    : parseFeedItems(body, source, policy)
  ).slice(0, 50);
  for (const item of items) {
    await env.DB.prepare(`
      INSERT OR IGNORE INTO source_items (
        id, source_id, source_type, title, canonical_url, external_id, author,
        published_at, excerpt, content_type, hash, thumbnail_url, rights_policy_json,
        raw_metadata_json, fetched_at, created_at, updated_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
    `).bind(
      item.id,
      source.id,
      source.source_type,
      item.title,
      item.canonicalUrl,
      item.externalId,
      item.author,
      item.publishedAt,
      item.excerpt,
      'text/html',
      item.hash,
      item.thumbnailUrl,
      JSON.stringify(policy),
      JSON.stringify({ scheduled: true })
    ).run();
  }

  await markSourceRefreshed(
    env,
    source,
    nextFetchAt,
    response.headers.get('ETag') || source.etag,
    response.headers.get('Last-Modified') || source.last_modified
  );
}

async function markSourceRefreshed(env, source, nextFetchAt, etag, lastModified) {
  await env.DB.prepare(`
    UPDATE source_subscriptions
    SET status = 'active',
        last_fetched_at = CURRENT_TIMESTAMP,
        next_fetch_at = ?,
        etag = COALESCE(?, etag),
        last_modified = COALESCE(?, last_modified),
        last_error = NULL,
        error_count = 0,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = ?
  `).bind(nextFetchAt, etag || null, lastModified || null, source.id).run();
}

function parseFeedItems(xml, source, policy) {
  const rssItems = blocks(xml, 'item');
  if (rssItems.length > 0) {
    return rssItems.map((item) => normalizeFeedBlock(item, source, policy, 'rss'));
  }
  return blocks(xml, 'entry').map((item) => normalizeFeedBlock(item, source, policy, 'atom'));
}

function parseApiItems(body, source, policy) {
  const value = JSON.parse(body);
  const rows = Array.isArray(value.articles) ? value.articles : Array.isArray(value.items) ? value.items : [];
  return rows.map((row) => normalizeApiItem(row, source, policy));
}

function normalizeApiItem(row, source, policy) {
  const title = String(row.title || '(untitled source item)').trim();
  const canonicalUrl = row.url || row.external_url || null;
  const externalId = row.id || row.guid || canonicalUrl || title;
  const author = row.author || row.byline || (row.source && row.source.name) || null;
  const publishedAt = normalizeDate(row.publishedAt || row.date_published || row.published_at);
  const excerptSource = row.description || row.summary || row.excerpt || '';
  const excerpt = excerptSource
    ? excerptText(excerptSource, policy.full_text_allowed && !policy.excerpt_only ? 2000 : 800)
    : null;
  const seed = `${source.id}\n${externalId}\n${canonicalUrl || ''}\n${title}`;
  const hash = stableId(seed);
  return {
    id: `src-${hash.slice(0, 24)}`,
    title,
    canonicalUrl,
    externalId,
    author,
    publishedAt,
    excerpt,
    thumbnailUrl: policy.no_image ? null : (row.urlToImage || row.image || null),
    hash,
  };
}

function normalizeFeedBlock(block, source, policy, kind) {
  const title = textTag(block, 'title') || '(untitled source item)';
  const canonicalUrl = kind === 'atom'
    ? attrTag(block, 'link', 'href') || textTag(block, 'link')
    : textTag(block, 'link');
  const externalId = textTag(block, 'guid') || textTag(block, 'id') || canonicalUrl || title;
  const author = textTag(block, 'author') || textTag(block, 'dc:creator') || textTag(block, 'name');
  const publishedAt = normalizeDate(textTag(block, 'pubDate') || textTag(block, 'published') || textTag(block, 'updated'));
  const rawExcerpt = textTag(block, 'description') || textTag(block, 'summary') || '';
  const excerpt = rawExcerpt ? excerptText(rawExcerpt, policy.full_text_allowed && !policy.excerpt_only ? 2000 : 800) : null;
  const seed = `${source.id}\n${externalId}\n${canonicalUrl || ''}\n${title}`;
  const hash = stableId(seed);
  return {
    id: `src-${hash.slice(0, 24)}`,
    title,
    canonicalUrl: canonicalUrl || null,
    externalId: externalId || null,
    author: author || null,
    publishedAt,
    excerpt,
    thumbnailUrl: policy.no_image ? null : attrTag(block, 'media:thumbnail', 'url'),
    hash,
  };
}

function blocks(xml, tag) {
  const re = new RegExp(`<${escapeRegex(tag)}\\b[^>]*>([\\s\\S]*?)<\\/${escapeRegex(tag)}>`, 'gi');
  return Array.from(xml.matchAll(re)).map((match) => match[1]);
}

function textTag(xml, tag) {
  const re = new RegExp(`<${escapeRegex(tag)}\\b[^>]*>([\\s\\S]*?)<\\/${escapeRegex(tag)}>`, 'i');
  const match = xml.match(re);
  return match ? decodeXml(stripCdata(match[1]).replace(/<[^>]*>/g, ' ').trim()) : null;
}

function attrTag(xml, tag, attr) {
  const re = new RegExp(`<${escapeRegex(tag)}\\b([^>]*)>`, 'i');
  const match = xml.match(re);
  if (!match) return null;
  const attrRe = new RegExp(`${escapeRegex(attr)}=["']([^"']+)["']`, 'i');
  const attrMatch = match[1].match(attrRe);
  return attrMatch ? decodeXml(attrMatch[1]) : null;
}

function parsePolicy(value) {
  try {
    return Object.assign({
      private_reader_only: true,
      excerpt_only: true,
      link_required: true,
      attribution_required: true,
      no_image: false,
      full_text_allowed: false,
    }, JSON.parse(value || '{}'));
  } catch {
    return {
      private_reader_only: true,
      excerpt_only: true,
      link_required: true,
      attribution_required: true,
      no_image: false,
      full_text_allowed: false,
    };
  }
}

function normalizeDate(value) {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function excerptText(value, maxChars) {
  return decodeXml(value)
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, maxChars);
}

function stripCdata(value) {
  return value.replace(/^<!\[CDATA\[/, '').replace(/\]\]>$/, '');
}

function decodeXml(value) {
  return value
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function handleMedia(request, env, path) {
  const mediaPath = decodeURIComponent(path.substring(7));

  if (!mediaPath) {
    return new Response('Missing filename', { status: 400 });
  }
  const privateMedia = mediaPath.startsWith('_private/');
  if (mediaPath.startsWith('private/')) {
    return new Response('Not Found', { status: 404 });
  }
  const filename = privateMedia ? `private/${mediaPath.slice('_private/'.length)}` : mediaPath;

  try {
    // Fetch from R2 bucket
    const object = await env.MEDIA_BUCKET.get(filename);

    if (!object) {
      return new Response('Not Found', { status: 404 });
    }

    // Determine Content-Type from file extension
    const ext = filename.split('.').pop().toLowerCase();
    const contentTypes = {
      'jpg': 'image/jpeg',
      'jpeg': 'image/jpeg',
      'png': 'image/png',
      'gif': 'image/gif',
      'webp': 'image/webp',
      'mp4': 'video/mp4',
      'webm': 'video/webm',
    };
    const contentType = contentTypes[ext] || 'application/octet-stream';

    // Return with proper headers and caching
    const headers = new Headers();
    headers.set('Content-Type', contentType);
    headers.set('Cache-Control', privateMedia ? 'private, no-store' : 'public, max-age=31536000, immutable');
    headers.set('Access-Control-Allow-Origin', '*');

    return new Response(object.body, { headers });
  } catch (error) {
    console.error('R2 fetch error:', error);
    return new Response('Internal Server Error', { status: 500 });
  }
}

async function handleMastodonApi(request, env, url) {
  const path = url.pathname;

  if (request.method === 'OPTIONS') {
    return apiJson({}, 204);
  }

  if (request.method === 'GET' && (path === '/api/v1/instance' || path === '/api/v2/instance')) {
    const instance = {
      uri: 'social.dais.social',
      domain: 'social.dais.social',
      title: 'dais',
      short_description: 'Private-by-default single-user social server',
      description: 'dais speaks ActivityPub and AT Protocol with private-by-default posting.',
      email: '',
      version: '4.2.0 (compatible; dais)',
      registrations: false,
      approval_required: true,
      invites_enabled: false,
      urls: { streaming_api: 'wss://social.dais.social' },
      stats: { user_count: 1, status_count: await publicStatusCount(env), domain_count: 1 },
    };
    if (path === '/api/v2/instance') {
      return apiJson({
        ...instance,
        source_url: 'https://github.com/marctjones/dais',
        languages: ['en'],
        configuration: {
          statuses: { max_characters: 5000, max_media_attachments: 4, characters_reserved_per_url: 23 },
          media_attachments: {
            supported_mime_types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'video/mp4', 'video/webm'],
          },
          polls: { max_options: 4, max_characters_per_option: 200, min_expiration: 300, max_expiration: 2629746 },
        },
      });
    }
    return apiJson(instance);
  }

  if (request.method === 'POST' && path === '/api/v1/apps') {
    let body = {};
    try {
      body = await request.json();
    } catch {
      body = {};
    }
    const name = body.client_name || body.name || 'dais client';
    const redirectUri = body.redirect_uris || body.redirect_uri || 'urn:ietf:wg:oauth:2.0:oob';
    return apiJson({
      id: stableId(name),
      name,
      website: body.website || null,
      redirect_uri: redirectUri,
      client_id: `dais-${stableId(name)}`,
      client_secret: `dais-secret-${stableId(redirectUri)}`,
      vapid_key: '',
    });
  }

  if (request.method === 'GET' && path === '/oauth/authorize') {
    const redirectUri = url.searchParams.get('redirect_uri');
    const state = url.searchParams.get('state');
    const code = 'dais-local-owner';
    if (redirectUri && redirectUri !== 'urn:ietf:wg:oauth:2.0:oob') {
      const redirect = new URL(redirectUri);
      redirect.searchParams.set('code', code);
      if (state) redirect.searchParams.set('state', state);
      return Response.redirect(redirect.toString(), 302);
    }
    return new Response(`Authorization code: ${code}\n`, {
      headers: textHeaders('text/plain; charset=utf-8'),
    });
  }

  if (request.method === 'POST' && path === '/oauth/token') {
    // Do not mint or reveal the production owner API token. Until dais has a
    // real user-consent OAuth screen, Mastodon clients authenticate with an
    // owner-provisioned bearer token.
    return apiJson({
      access_token: 'owner-token-required',
      token_type: 'Bearer',
      scope: 'read write follow push',
      created_at: Math.floor(Date.now() / 1000),
    });
  }

  if (request.method === 'POST' && path === '/oauth/revoke') {
    return apiJson({});
  }

  if (request.method === 'GET' && path === '/api/v1/preferences') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const settings = await ownerSettings(env);
    return apiJson({
      'posting:default:visibility': mastodonVisibility(settings.default_visibility || 'followers'),
      'posting:default:sensitive': false,
      'posting:default:language': 'en',
      'reading:expand:media': 'default',
      'reading:expand:spoilers': false,
    });
  }

  if (request.method === 'GET' && path === '/api/v1/custom_emojis') {
    return apiJson([]);
  }

  if (request.method === 'GET' && path === '/api/v1/markers') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson({});
  }

  if (request.method === 'POST' && path === '/api/v1/markers') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson({});
  }

  if (request.method === 'GET' && path === '/api/v1/accounts/verify_credentials') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonAccount(env));
  }

  if (request.method === 'PATCH' && path === '/api/v1/accounts/update_credentials') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const body = await readRequestBody(request);
    const profile = {};
    if (body.display_name !== undefined) profile.display_name = body.display_name;
    if (body.note !== undefined) profile.summary = body.note;
    if (Object.keys(profile).length > 0) await ownerUpdateProfile(env, profile);
    return apiJson(await mastodonAccount(env));
  }

  if (request.method === 'GET' && path === '/api/v1/accounts/relationships') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const ids = url.searchParams.getAll('id[]').concat(url.searchParams.getAll('id'));
    return apiJson(await mastodonRelationships(env, ids));
  }

  if (request.method === 'GET' && path === '/api/v1/blocks') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonBlocks(env, clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && path === '/api/v1/mutes') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson([]);
  }

  if (request.method === 'GET' && path === '/api/v1/timelines/public') {
    return apiJson(await mastodonStatuses(env, clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && path === '/api/v1/timelines/home') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonStatuses(env, clampLimit(url.searchParams.get('limit'))));
  }

  const accountStatuses = path.match(/^\/api\/v1\/accounts\/([^/]+)\/statuses$/);
  if (request.method === 'GET' && accountStatuses) {
    return apiJson(await mastodonStatuses(env, clampLimit(url.searchParams.get('limit'))));
  }

  const accountFollowers = path.match(/^\/api\/v1\/accounts\/([^/]+)\/followers$/);
  if (request.method === 'GET' && accountFollowers) {
    return apiJson(await mastodonFollowers(env, clampLimit(url.searchParams.get('limit'))));
  }

  const accountFollowing = path.match(/^\/api\/v1\/accounts\/([^/]+)\/following$/);
  if (request.method === 'GET' && accountFollowing) {
    return apiJson(await mastodonFollowing(env, clampLimit(url.searchParams.get('limit'))));
  }

  const account = path.match(/^\/api\/v1\/accounts\/([^/]+)$/);
  if (request.method === 'GET' && account) {
    return apiJson(await mastodonAccount(env));
  }

  const accountAction = path.match(/^\/api\/v1\/accounts\/([^/]+)\/(follow|unfollow|block|unblock|mute|unmute)$/);
  if (request.method === 'POST' && accountAction) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const id = decodeURIComponent(accountAction[1]);
    const action = accountAction[2];
    if (action === 'follow') {
      try {
        await ownerFollowActor(env, id);
      } catch (error) {
        return apiJson({ error: error.message || 'follow failed' }, 400);
      }
    } else if (action === 'unfollow') {
      try {
        await ownerUnfollowActor(env, id);
      } catch {
        // Keep compatibility shape idempotent for clients refreshing relationship state.
      }
    } else if (action === 'block') {
      await mastodonSetBlock(env, id, true);
    } else if (action === 'unblock') {
      await mastodonSetBlock(env, id, false);
    }
    return apiJson((await mastodonRelationships(env, [id]))[0]);
  }

  if (request.method === 'GET' && path === '/api/v1/favourites') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonStatusesByInteraction(env, 'like', clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && path === '/api/v1/bookmarks') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson([]);
  }

  if (request.method === 'GET' && path === '/api/v1/conversations') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonConversations(env, clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && (path === '/api/v1/search' || path === '/api/v2/search')) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonSearch(env, url.searchParams.get('q') || '', clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && (path === '/api/v1/filters' || path === '/api/v2/filters')) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson([]);
  }

  if (request.method === 'GET' && path === '/api/v1/lists') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson([]);
  }

  const statusContext = path.match(/^\/api\/v1\/statuses\/(.+)\/context$/);
  if (request.method === 'GET' && statusContext) {
    const value = await mastodonStatusContext(env, decodeURIComponent(statusContext[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(value);
  }

  const statusAction = path.match(/^\/api\/v1\/statuses\/(.+)\/(favourite|unfavourite|reblog|unreblog)$/);
  if (request.method === 'POST' && statusAction) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const statusId = decodeURIComponent(statusAction[1]);
    const value = await mastodonStatus(env, statusId);
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    await mastodonToggleStatusInteraction(env, statusId, statusAction[2]);
    return apiJson(await mastodonStatus(env, statusId) || value);
  }

  const status = path.match(/^\/api\/v1\/statuses\/([^/]+)$/);
  if (request.method === 'GET' && status) {
    const value = await mastodonStatus(env, decodeURIComponent(status[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(value);
  }

  if ((request.method === 'PUT' || request.method === 'PATCH') && status) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const body = await readRequestBody(request);
    const updated = await mastodonUpdateStatus(env, decodeURIComponent(status[1]), {
      text: String(body.status || body.text || '').trim(),
      summary: String(body.spoiler_text || '').trim() || null,
    });
    if (!updated) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(updated);
  }

  if (request.method === 'DELETE' && status) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const deleted = await mastodonDeleteStatus(env, decodeURIComponent(status[1]));
    if (!deleted) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(deleted);
  }

  if (request.method === 'POST' && path === '/api/v1/statuses') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const body = await readRequestBody(request);
    const text = String(body.status || body.text || '').trim();
    if (!text) return apiJson({ error: 'status is required' }, 400);
    const visibility = normalizeMastodonVisibility(body.visibility || 'private') || 'followers';
    const mediaIds = requestBodyArray(body, 'media_ids');
    let poll = null;
    try {
      poll = mastodonPollFromRequestBody(body);
    } catch (error) {
      return apiJson({ error: error.message || 'invalid poll' }, 400);
    }
    const created = await ownerCreatePost(env, {
      text,
      visibility,
      protocol: 'activitypub',
      attachments: await mastodonAttachmentsForMediaIds(env, mediaIds, visibility),
      inReplyTo: optionalString(body.in_reply_to_id),
      objectType: poll ? 'Question' : 'Note',
      pollOptions: poll,
      summary: optionalString(body.spoiler_text),
    });
    return apiJson(statusJson({
      id: created.id,
      actor_id: created.actor_id,
      content: created.content,
      content_html: created.content_html,
      object_type: created.object_type,
      name: null,
      summary: created.summary,
      visibility: created.visibility,
      published_at: created.published_at,
      in_reply_to: body.in_reply_to_id || null,
      media_attachments: created.attachments?.length ? JSON.stringify(created.attachments) : null,
      poll_options: created.poll_options,
    }, await mastodonAccount(env)), 201);
  }

  if (request.method === 'POST' && (path === '/api/v1/media' || path === '/api/v2/media')) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    try {
      return apiJson(await mastodonUploadMedia(request, env), 200);
    } catch (error) {
      return apiJson({ error: error.message || 'media upload failed' }, 400);
    }
  }

  if (request.method === 'GET' && path === '/api/v1/notifications') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    return apiJson(await mastodonNotifications(env, clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'POST' && path === '/api/v1/notifications/clear') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    await env.DB.prepare("UPDATE notifications SET read = 1").run();
    return apiJson({});
  }

  const notificationDismiss = path.match(/^\/api\/v1\/notifications\/([^/]+)\/dismiss$/);
  if (request.method === 'POST' && notificationDismiss) {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    await env.DB.prepare("UPDATE notifications SET read = 1 WHERE id = ?1").bind(decodeURIComponent(notificationDismiss[1])).run();
    return apiJson({});
  }

  if (request.method === 'POST' && path === '/api/v1/reports') {
    const auth = requireBearer(request, env);
    if (auth) return auth;
    const body = await readRequestBody(request);
    return apiJson({
      id: `report-${stableId(`${body.account_id || ''}\n${Date.now()}`)}`,
      action_taken: false,
      action_taken_at: null,
      category: body.category || 'other',
      comment: body.comment || '',
      forwarded: false,
      created_at: new Date().toISOString(),
      status_ids: requestBodyArray(body, 'status_ids'),
      rules: [],
      target_account: body.account_id ? remoteAccountJson({ actor_id: body.account_id, url: body.account_id }) : null,
    }, 201);
  }

  if (request.method === 'GET' && path.startsWith('/api/v1/streaming')) {
    return new Response('', {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Access-Control-Allow-Origin': '*',
      },
    });
  }

  return apiJson({ error: 'Not implemented in dais Mastodon compatibility API' }, 404);
}

function oauthAuthorizationServerMetadata(url) {
  const origin = url.origin;
  return apiJson({
    issuer: origin,
    authorization_endpoint: `${origin}/oauth/authorize`,
    token_endpoint: `${origin}/oauth/token`,
    revocation_endpoint: `${origin}/oauth/revoke`,
    scopes_supported: ['read', 'write', 'follow', 'push'],
    response_types_supported: ['code'],
    grant_types_supported: ['authorization_code'],
    token_endpoint_auth_methods_supported: ['client_secret_post', 'client_secret_basic', 'none'],
    code_challenge_methods_supported: ['S256', 'plain'],
    service_documentation: 'https://github.com/marctjones/dais',
  });
}

function nodeInfoDiscovery(url) {
  return apiJson({
    links: [
      {
        rel: 'http://nodeinfo.diaspora.software/ns/schema/2.0',
        href: `${url.origin}/nodeinfo/2.0`,
      },
    ],
  });
}

async function nodeInfoDocument(env) {
  return apiJson({
    version: '2.0',
    software: {
      name: 'dais',
      version: '1.28',
      repository: 'https://github.com/marctjones/dais',
    },
    protocols: ['activitypub'],
    services: {
      inbound: [],
      outbound: [],
    },
    openRegistrations: false,
    usage: {
      users: {
        total: 1,
        activeMonth: 1,
        activeHalfyear: 1,
      },
      localPosts: await publicStatusCount(env),
    },
    metadata: {
      nodeName: 'dais',
      privateByDefault: true,
    },
  });
}

async function handleOwnerApi(request, env, url) {
  const path = url.pathname.replace(/^\/api\/dais\/owner/, '') || '/';

  if (request.method === 'OPTIONS') {
    return apiJson({}, 204);
  }

  const auth = requireOwnerBearer(request, env);
  if (auth) return auth;

  if (request.method === 'GET' && path === '/snapshot') {
    return apiJson(await ownerSnapshot(env));
  }

  if (request.method === 'GET' && path === '/profile') {
    return apiJson(await ownerProfile(env));
  }

  if (request.method === 'POST' && path === '/profile') {
    const body = await readJson(request);
    let profile;
    try {
      profile = await ownerUpdateProfile(env, body);
    } catch (error) {
      return apiJson({ error: error.message || 'profile update failed' }, 400);
    }
    return apiJson(profile);
  }

  if (request.method === 'POST' && path === '/media') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerUploadMedia(env, body), 201);
    } catch (error) {
      return apiJson({ error: error.message || 'media upload failed' }, 400);
    }
  }

  if (request.method === 'GET' && path === '/posts') {
    return apiJson({
      items: await ownerPosts(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  const postDetail = path.match(/^\/posts\/(.+)$/);
  if (request.method === 'GET' && postDetail) {
    const post = await ownerPostDetail(env, decodeURIComponent(postDetail[1]));
    if (!post) return apiJson({ error: 'post not found' }, 404);
    return apiJson(post);
  }

  if (request.method === 'GET' && path === '/timeline/home') {
    return apiJson({
      items: await ownerHomeTimeline(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  if (request.method === 'GET' && path === '/followers') {
    return apiJson({
      items: await ownerFollowers(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  if (request.method === 'POST' && path === '/followers/status') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerSetFollowerStatus(env, {
        followerActorId: String(body.follower_actor_id || '').trim(),
        status: String(body.status || '').trim().toLowerCase(),
      }));
    } catch (error) {
      return apiJson({ error: error.message }, 400);
    }
  }

  if (request.method === 'GET' && path === '/following') {
    return apiJson({
      items: await ownerFollowing(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  if (request.method === 'POST' && path === '/discovery/actor') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerDiscoverActor(env, String(body.target || '').trim()));
    } catch (error) {
      return apiJson({ error: error.message || 'actor discovery failed' }, 400);
    }
  }

  if (request.method === 'POST' && path === '/following/follow') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerFollowActor(env, String(body.target || '').trim()), 201);
    } catch (error) {
      return apiJson({ error: error.message || 'follow failed' }, 400);
    }
  }

  if (request.method === 'POST' && path === '/following/unfollow') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerUnfollowActor(env, String(body.target || '').trim()));
    } catch (error) {
      return apiJson({ error: error.message || 'unfollow failed' }, 400);
    }
  }

  if (request.method === 'GET' && path === '/notifications') {
    return apiJson({
      items: await ownerNotifications(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  if (request.method === 'POST' && path === '/notifications/read') {
    const body = await readJson(request);
    if (!body.id) return apiJson({ error: 'id is required' }, 400);
    await env.DB.prepare("UPDATE notifications SET read = 1 WHERE id = ?1").bind(String(body.id)).run();
    return apiJson({ ok: true });
  }

  if (request.method === 'GET' && path === '/deliveries') {
    return apiJson({
      items: await ownerDeliveries(env, clampLimit(url.searchParams.get('limit'))),
    });
  }

  if (request.method === 'GET' && path === '/sources') {
    return apiJson({
      subscriptions: await ownerSourceSubscriptions(env, clampLimit(url.searchParams.get('limit'))),
      items: await ownerSourceItems(env, clampLimit(url.searchParams.get('items_limit') || '40')),
    });
  }

  if (request.method === 'POST' && path === '/sources') {
    const body = await readJson(request);
    try {
      const source = await ownerAddSource(env, body);
      return apiJson({ ok: true, source }, 201);
    } catch (error) {
      return apiJson({ error: error.message || 'source add failed' }, 400);
    }
  }

  if (request.method === 'DELETE' && path.startsWith('/sources/')) {
    const id = decodeURIComponent(path.slice('/sources/'.length));
    if (!id) return apiJson({ error: 'id is required' }, 400);
    await env.DB.prepare("DELETE FROM source_subscriptions WHERE id = ?1").bind(id).run();
    return apiJson({ ok: true });
  }

  if (request.method === 'POST' && path === '/sources/refresh') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerRefreshSources(env, optionalString(body.id)));
    } catch (error) {
      return apiJson({ error: error.message || 'source refresh failed' }, 400);
    }
  }

  if (request.method === 'GET' && path === '/moderation') {
    return apiJson(await ownerModeration(env));
  }

  if (request.method === 'POST' && path === '/moderation/block') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerBlock(env, body), 201);
    } catch (error) {
      return apiJson({ error: error.message || 'block failed' }, 400);
    }
  }

  if (request.method === 'POST' && path === '/moderation/unblock') {
    const body = await readJson(request);
    const value = optionalString(body.value || body.actor_id || body.actorId || body.domain);
    if (!value) return apiJson({ error: 'value is required' }, 400);
    await env.DB.prepare("DELETE FROM blocks WHERE id = ?1 OR actor_id = ?1 OR blocked_domain = ?1").bind(value).run();
    return apiJson({ ok: true });
  }

  if (request.method === 'POST' && path === '/moderation/allowlist') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerAllowHost(env, body), 201);
    } catch (error) {
      return apiJson({ error: error.message || 'allowlist update failed' }, 400);
    }
  }

  if (request.method === 'DELETE' && path.startsWith('/moderation/allowlist/')) {
    const host = normalizeHost(decodeURIComponent(path.slice('/moderation/allowlist/'.length)));
    await env.DB.prepare("DELETE FROM federation_allowlist WHERE host = ?1").bind(host).run();
    return apiJson({ ok: true });
  }

  if (request.method === 'GET' && path === '/diagnostics') {
    return apiJson({
      items: await ownerDiagnostics(env),
    });
  }

  if (request.method === 'POST' && path === '/posts') {
    const body = await readJson(request);
    const text = String(body.text || body.content || '').trim();
    if (!text) return apiJson({ error: 'text is required' }, 400);

    const visibility = normalizeVisibility(body.visibility || 'followers');
    if (!visibility) return apiJson({ error: 'unsupported visibility' }, 400);
    const protocol = normalizeProtocol(body.protocol || 'activitypub');
    if (!protocol) return apiJson({ error: 'unsupported protocol' }, 400);
    if ((visibility === 'followers' || visibility === 'direct') && protocol === 'atproto') {
      return apiJson({ error: 'private posts cannot route only to atproto' }, 400);
    }

    const recipients = Array.isArray(body.recipients)
      ? body.recipients.map((value) => String(value).trim()).filter(Boolean)
      : [];
    const attachments = Array.isArray(body.attachments) ? body.attachments : [];
    const encrypt = Boolean(body.encrypt);
    const inReplyTo = optionalString(body.in_reply_to || body.inReplyTo || body.in_reply_to_id);
    if (visibility === 'direct' && recipients.length === 0) {
      return apiJson({ error: 'direct posts require at least one recipient' }, 400);
    }

    let created;
    try {
      created = await ownerCreatePost(env, { text, visibility, protocol, recipients, attachments, encrypt, inReplyTo });
    } catch (error) {
      return apiJson({ error: error.message || 'post creation failed' }, 400);
    }
    return apiJson(created, 201);
  }

  if (request.method === 'POST' && path === '/interactions') {
    const body = await readJson(request);
    try {
      return apiJson(await ownerPublishInteraction(env, {
        objectId: String(body.object_id || body.objectId || '').trim(),
        interaction: String(body.interaction || body.action || '').trim().toLowerCase(),
      }), 201);
    } catch (error) {
      return apiJson({ error: error.message || 'interaction failed' }, 400);
    }
  }

  return apiJson({ error: 'Not implemented in dais owner API' }, 404);
}

async function ownerSnapshot(env) {
  const [profile, homeTimeline, posts, followers, following, sources, moderation, diagnostics] = await Promise.all([
    ownerProfile(env),
    ownerHomeTimeline(env, 20),
    ownerPosts(env, 20),
    ownerFollowers(env, 100),
    ownerFollowing(env, 100),
    ownerSourceItems(env, 20),
    ownerModeration(env),
    ownerDiagnostics(env),
  ]);
  const settings = await ownerSettings(env);
  return {
    settings: {
      instance_url: 'https://social.dais.social',
      owner_token_present: true,
      default_visibility: titleVisibility(settings.default_visibility || 'followers'),
      default_protocol: 'Both',
    },
    active_section: 'Home',
    profile,
    home_timeline: homeTimeline.map((post) => ({
      id: post.id,
      object_id: post.object_id,
      actor_id: post.actor_id,
      actor_username: post.actor_username || null,
      actor_display_name: post.actor_display_name || null,
      actor_avatar_url: post.actor_avatar_url || null,
      content: post.content || '',
      content_html: post.content_html || null,
      visibility: post.visibility || 'public',
      in_reply_to: post.in_reply_to || null,
      published_at: post.published_at || null,
      protocol: post.protocol || 'activitypub',
      reply_count: Number(post.reply_count || 0),
      like_count: Number(post.like_count || 0),
      boost_count: Number(post.boost_count || 0),
    })),
    posts: posts.map((post) => ({
      id: post.id,
      title: post.name || null,
      content: post.content || '',
      visibility: titleVisibility(post.visibility),
      protocol: titleProtocol(post.protocol),
      encrypted: Boolean(post.encrypted_message),
      attachments: parseAttachmentArray(post.media_attachments),
      reply_count: Number(post.reply_count || 0),
      like_count: Number(post.like_count || 0),
      boost_count: Number(post.boost_count || 0),
      published_at: post.published_at || null,
    })),
    followers,
    following,
    sources,
    moderation,
    diagnostics,
  };
}

async function ownerProfile(env) {
  const row = await env.DB.prepare(
    `SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type,
            display_name, summary, icon, image, avatar_url, header_url
     FROM actors
     WHERE username = 'social'
     LIMIT 1`,
  ).first();
  const username = row?.username || 'social';
  const actorUrl = row?.id || 'https://social.dais.social/users/social';
  const handleDomain = env.DOMAIN || 'dais.social';
  return {
    id: actorUrl,
    username,
    actor_type: row?.actor_type || 'Person',
    display_name: row?.display_name || null,
    summary: row?.summary || null,
    icon: row?.icon || null,
    image: row?.image || null,
    avatar_url: row?.avatar_url || row?.icon || null,
    header_url: row?.header_url || row?.image || null,
    public_handle: `@${username}@${handleDomain}`,
    actor_url: actorUrl,
  };
}

async function ownerUpdateProfile(env, body) {
  const assignments = ['updated_at = CURRENT_TIMESTAMP'];
  const values = [];
  const actorType = optionalString(body.actor_type);
  if (actorType) {
    if (!['Person', 'Group', 'Organization'].includes(actorType)) {
      throw new Error('actor_type must be Person, Group, or Organization');
    }
    values.push(actorType);
    assignments.push(`actor_type = ?${values.length}`);
  }
  for (const [column, value] of [
    ['display_name', optionalString(body.display_name)],
    ['summary', optionalString(body.summary)],
    ['icon', optionalUrl(body.icon, 'icon')],
    ['image', optionalUrl(body.image, 'image')],
  ]) {
    if (value !== null) {
      values.push(value);
      assignments.push(`${column} = ?${values.length}`);
      if (column === 'icon') {
        values.push(value);
        assignments.push(`avatar_url = ?${values.length}`);
      }
      if (column === 'image') {
        values.push(value);
        assignments.push(`header_url = ?${values.length}`);
      }
    }
  }
  if (assignments.length === 1) {
    throw new Error('no profile fields provided');
  }
  values.push('social');
  await env.DB.prepare(
    `UPDATE actors
     SET ${assignments.join(', ')}
     WHERE username = ?${values.length}`,
  ).bind(...values).run();
  return ownerProfile(env);
}

async function ownerSettings(env) {
  return await env.DB.prepare(
    `SELECT default_visibility, require_authorized_fetch, manually_approves_followers,
            COALESCE(closed_network, 0) AS closed_network
     FROM instance_settings
     WHERE id = 1`,
  ).first() || {
    default_visibility: 'followers',
    require_authorized_fetch: 1,
    manually_approves_followers: 1,
    closed_network: 0,
  };
}

async function ownerPosts(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
            atproto_uri, atproto_cid, encrypted_message, media_attachments,
            published_at, created_at, updated_at,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
     FROM posts
     ORDER BY published_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerPostDetail(env, id) {
  const post = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
            atproto_uri, atproto_cid, encrypted_message, media_attachments,
            published_at, created_at, updated_at, in_reply_to,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
     FROM posts
     WHERE id = ?1
     LIMIT 1`,
  ).bind(id).first();
  if (!post) return null;
  const [replies, likes, boosts] = await Promise.all([
    ownerPostReplies(env, id),
    ownerPostInteractions(env, id, 'like'),
    ownerPostInteractions(env, id, 'boost'),
  ]);
  return {
    id: post.id,
    actor_id: post.actor_id,
    title: post.name || null,
    content: post.content || '',
    content_html: post.content_html || null,
    visibility: titleVisibility(post.visibility),
    protocol: titleProtocol(post.protocol),
    encrypted: Boolean(post.encrypted_message),
    attachments: parseAttachmentArray(post.media_attachments),
    in_reply_to: post.in_reply_to || null,
    published_at: post.published_at || null,
    reply_count: Number(post.reply_count || 0),
    like_count: Number(post.like_count || 0),
    boost_count: Number(post.boost_count || 0),
    replies,
    likes,
    boosts,
  };
}

async function ownerPostReplies(env, id) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
            content, published_at, created_at
     FROM replies
     WHERE post_id = ?1 AND (hidden IS NULL OR hidden = 0)
     ORDER BY published_at ASC`,
  ).bind(id).all();
  return rows.results || [];
}

async function ownerPostInteractions(env, id, type) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
            object_url, created_at
     FROM interactions
     WHERE (post_id = ?1 OR object_url = ?1) AND type = ?2
     ORDER BY created_at DESC`,
  ).bind(id, type).all();
  return rows.results || [];
}

async function ownerHomeTimeline(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, object_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
            content, content_html, visibility, in_reply_to, published_at, updated_at,
            protocol, created_at,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = timeline_posts.object_id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'boost') AS boost_count
     FROM timeline_posts
     WHERE deleted_at IS NULL
     ORDER BY published_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerFollowers(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox,
            status, created_at, updated_at
     FROM followers
     ORDER BY
       CASE status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
       updated_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerSetFollowerStatus(env, { followerActorId, status }) {
  if (!followerActorId) throw new Error('follower_actor_id is required');
  if (!['approved', 'pending', 'rejected'].includes(status)) {
    throw new Error('status must be approved, pending, or rejected');
  }

  const localActor = await ownerLocalActor(env);
  const existing = await env.DB.prepare(
    `SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox, status
     FROM followers
     WHERE actor_id = ?1 AND follower_actor_id = ?2
     LIMIT 1`,
  ).bind(localActor.id, followerActorId).first();
  if (!existing) throw new Error('follower not found');

  await env.DB.prepare(
    `UPDATE followers
     SET status = ?1,
         updated_at = CURRENT_TIMESTAMP
     WHERE actor_id = ?2 AND follower_actor_id = ?3`,
  ).bind(status, localActor.id, followerActorId).run();

  let deliveryIds = [];
  if (status === 'approved') {
    const followId = existing.id || followerActorId;
    const acceptId = `${localActor.id}#accepts/${stableId(followId).slice(0, 16)}`;
    const activity = {
      '@context': 'https://www.w3.org/ns/activitystreams',
      id: acceptId,
      type: 'Accept',
      actor: localActor.id,
      to: [followerActorId],
      object: {
        id: followId,
        type: 'Follow',
        actor: followerActorId,
        object: localActor.id,
      },
    };
    const inbox = existing.follower_shared_inbox || existing.follower_inbox;
    deliveryIds = await insertDeliveryRows(env, acceptId, [inbox], 'Accept', JSON.stringify(activity));
  }

  return { ok: true, delivery_ids: deliveryIds };
}

async function ownerUploadMedia(env, body) {
  const filename = optionalString(body.filename);
  const dataBase64 = optionalString(body.data_base64);
  const mediaType = optionalString(body.media_type) || mediaTypeForFilename(filename || '');
  const access = optionalString(body.access) || 'public';
  if (!filename) throw new Error('filename is required');
  if (!dataBase64) throw new Error('data_base64 is required');
  if (!allowedMediaType(mediaType)) throw new Error('unsupported media type');
  if (!['public', 'private'].includes(access)) throw new Error('access must be public or private');

  const bytes = base64ToBytes(dataBase64);
  if (bytes.byteLength > 8 * 1024 * 1024) {
    throw new Error('media file is larger than 8 MB');
  }

  const safeName = safeMediaFilename(filename);
  const timestamp = new Date().toISOString().replace(/[-:TZ.]/g, '').slice(0, 14);
  const token = randomToken();
  const publicName = `${timestamp}-${stableId(`${safeName}\n${dataBase64}`).slice(0, 12)}-${safeName}`;
  const key = access === 'private' ? `private/${token}/${safeName}` : `uploads/${publicName}`;
  await env.MEDIA_BUCKET.put(key, bytes, {
    httpMetadata: { contentType: mediaType },
  });
  const url = access === 'private'
    ? `https://social.dais.social/media/_private/${token}/${safeName}`
    : `https://social.dais.social/media/${key}`;
  const attachment = {
    type: mediaType.startsWith('image/') ? 'Image' : 'Document',
    mediaType,
    url,
    name: safeName,
  };
  return { url, media_type: mediaType, access, attachment };
}

function randomToken() {
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  return Array.from(bytes).map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function base64ToBytes(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function safeMediaFilename(value) {
  const safe = String(value || '')
    .split(/[\\/]/)
    .pop()
    .replace(/[^A-Za-z0-9._-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^\.+/, '')
    .slice(0, 96);
  if (!safe) throw new Error('filename is invalid');
  return safe;
}

function mediaTypeForFilename(filename) {
  const ext = String(filename || '').split('.').pop().toLowerCase();
  const types = {
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    png: 'image/png',
    gif: 'image/gif',
    webp: 'image/webp',
    mp4: 'video/mp4',
    webm: 'video/webm',
  };
  return types[ext] || 'application/octet-stream';
}

function allowedMediaType(value) {
  return ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'video/mp4', 'video/webm'].includes(value);
}

async function ownerFollowing(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
     FROM following
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerFollowActor(env, target) {
  if (!target) throw new Error('target is required');
  const localActor = await ownerLocalActor(env);
  const remote = await resolveActivityPubActor(target);
  if (!remote.id || !remote.inbox) {
    throw new Error('target actor must expose id and inbox');
  }
  if (remote.id === localActor.id) {
    throw new Error('cannot follow the local actor');
  }
  const now = new Date().toISOString();
  const followId = `${localActor.id}#follows/${stableId(`${remote.id}\n${now}`).slice(0, 16)}`;
  const activity = {
    '@context': 'https://www.w3.org/ns/activitystreams',
    type: 'Follow',
    id: followId,
    actor: localActor.id,
    object: remote.id,
    to: [remote.id],
    published: now,
  };
  await env.DB.prepare(
    `INSERT INTO following (
       id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
     ) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL)
     ON CONFLICT(actor_id, target_actor_id) DO UPDATE SET
       id = excluded.id,
       target_inbox = excluded.target_inbox,
       status = 'pending',
       created_at = excluded.created_at,
       accepted_at = NULL`,
  ).bind(followId, localActor.id, remote.id, remote.inbox, now).run();
  const deliveryIds = await insertDeliveryRows(env, followId, [remote.inbox], 'Follow', JSON.stringify(activity));
  return {
    ok: true,
    following: await ownerFollowingRow(env, localActor.id, remote.id),
    delivery_ids: deliveryIds,
  };
}

async function ownerDiscoverActor(env, target) {
  if (!target) throw new Error('target is required');
  const localActor = await ownerLocalActor(env);
  const remote = await resolveActivityPubActor(target);
  const following = await ownerFollowingRow(env, localActor.id, remote.id);
  return {
    id: remote.id,
    inbox: remote.inbox,
    shared_inbox: remote.shared_inbox || null,
    preferred_username: remote.preferred_username || null,
    name: remote.name || null,
    summary: remote.summary || null,
    url: remote.url || null,
    icon_url: remote.icon_url || null,
    handle: actorHandle(remote),
    following_status: following?.status || null,
  };
}

async function ownerUnfollowActor(env, target) {
  if (!target) throw new Error('target is required');
  const localActor = await ownerLocalActor(env);
  const remote = await resolveActivityPubActor(target);
  const existing = await ownerFollowingRow(env, localActor.id, remote.id);
  if (!existing) throw new Error('not currently following target');
  const now = new Date().toISOString();
  const undoId = `${localActor.id}#undos/${stableId(`${remote.id}\n${now}`).slice(0, 16)}`;
  const followActivity = {
    type: 'Follow',
    id: existing.id,
    actor: localActor.id,
    object: remote.id,
  };
  const activity = {
    '@context': 'https://www.w3.org/ns/activitystreams',
    type: 'Undo',
    id: undoId,
    actor: localActor.id,
    object: followActivity,
    to: [remote.id],
    published: now,
  };
  await env.DB.prepare(
    `UPDATE following
     SET status = 'rejected', accepted_at = NULL
     WHERE actor_id = ?1 AND target_actor_id = ?2`,
  ).bind(localActor.id, remote.id).run();
  const deliveryIds = await insertDeliveryRows(env, undoId, [existing.target_inbox || remote.inbox], 'Undo', JSON.stringify(activity));
  return {
    ok: true,
    following: await ownerFollowingRow(env, localActor.id, remote.id),
    delivery_ids: deliveryIds,
  };
}

async function ownerPublishInteraction(env, { objectId, interaction }) {
  if (!objectId) throw new Error('object_id is required');
  publicHttpsUrl(objectId, 'object_id');
  const undo = interaction === 'unlike' || interaction === 'unboost';
  const normalized = interaction === 'unlike'
    ? 'like'
    : interaction === 'unboost'
      ? 'boost'
      : interaction;
  if (!['like', 'boost'].includes(normalized)) {
    throw new Error('interaction must be like, unlike, boost, or unboost');
  }
  const localActor = await ownerLocalActor(env);
  const targetInbox = await resolveActivityPubObjectInbox(objectId);
  const now = new Date().toISOString();
  const activityType = normalized === 'like' ? 'Like' : 'Announce';
  const activityId = `${localActor.id}#${normalized}s/${stableId(objectId).slice(0, 16)}`;
  const outgoingId = undo
    ? `${localActor.id}#undos/${normalized}/${stableId(`${objectId}\n${now}`).slice(0, 16)}`
    : activityId;
  const activity = undo
    ? {
        '@context': 'https://www.w3.org/ns/activitystreams',
        id: outgoingId,
        type: 'Undo',
        actor: localActor.id,
        published: now,
        to: ['https://www.w3.org/ns/activitystreams#Public'],
        cc: [`${localActor.id}/followers`],
        object: {
          id: activityId,
          type: activityType,
          actor: localActor.id,
          object: objectId,
        },
      }
    : {
        '@context': 'https://www.w3.org/ns/activitystreams',
        id: outgoingId,
        type: activityType,
        actor: localActor.id,
        published: now,
        to: ['https://www.w3.org/ns/activitystreams#Public'],
        cc: [`${localActor.id}/followers`],
        object: objectId,
      };
  const deliveryIds = await insertDeliveryRows(env, objectId, [targetInbox], undo ? 'Undo' : activityType, JSON.stringify(activity));
  if (undo) {
    await env.DB.prepare("DELETE FROM interactions WHERE id = ?1").bind(activityId).run();
  } else {
    await env.DB.prepare(
      `INSERT OR REPLACE INTO interactions (
         id, type, actor_id, object_url, created_at
       ) VALUES (?1, ?2, ?3, ?4, ?5)`,
    ).bind(activityId, normalized, localActor.id, objectId, now).run();
  }
  return {
    ok: true,
    activity_id: outgoingId,
    interaction: undo ? `undo-${normalized}` : normalized,
    object_id: objectId,
    delivery_ids: deliveryIds,
  };
}

async function ownerFollowingRow(env, actorId, targetActorId) {
  return await env.DB.prepare(
    `SELECT id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
     FROM following
     WHERE actor_id = ?1 AND target_actor_id = ?2
     LIMIT 1`,
  ).bind(actorId, targetActorId).first();
}

async function ownerLocalActor(env) {
  const row = await env.DB.prepare(
    "SELECT id, username FROM actors WHERE username = 'social' LIMIT 1",
  ).first();
  return {
    id: row?.id || 'https://social.dais.social/users/social',
    username: row?.username || 'social',
  };
}

async function resolveActivityPubActor(target) {
  const actorUrl = target.startsWith('http://') || target.startsWith('https://')
    ? publicHttpsUrl(target, 'target').toString()
    : await resolveWebfingerActor(target);
  const response = await fetch(actorUrl, {
    headers: {
      Accept: 'application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams"',
      'User-Agent': 'dais-owner-api/1.0',
    },
  });
  if (!response.ok) {
    throw new Error(`could not fetch actor ${actorUrl}: HTTP ${response.status}`);
  }
  const actor = await response.json();
  const inbox = String(actor.inbox || '').trim();
  return {
    id: String(actor.id || actorUrl).trim(),
    inbox,
    shared_inbox: actor.endpoints?.sharedInbox || null,
    preferred_username: actor.preferredUsername || null,
    name: actor.name || null,
    summary: actor.summary || null,
    icon_url: actor.icon?.url || null,
    url: actor.url || actorUrl,
  };
}

function actorHandle(actor) {
  if (!actor.preferred_username) return null;
  try {
    const url = new URL(actor.id || actor.url);
    return `@${actor.preferred_username}@${url.hostname}`;
  } catch {
    return null;
  }
}

async function resolveActivityPubObjectInbox(objectId) {
  const objectUrl = publicHttpsUrl(objectId, 'object_id').toString();
  const response = await fetch(objectUrl, {
    headers: {
      Accept: 'application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", application/json',
      'User-Agent': 'dais-owner-api/1.0',
    },
  });
  if (!response.ok) {
    throw new Error(`could not fetch object ${objectUrl}: HTTP ${response.status}`);
  }
  const object = await response.json();
  const actorId = String(object.attributedTo || object.actor || '').trim();
  if (!actorId) throw new Error('object does not expose attributedTo or actor');
  const actor = await resolveActivityPubActor(actorId);
  if (!actor.inbox) throw new Error('object actor does not expose inbox');
  return actor.shared_inbox || actor.inbox;
}

function isLocalObjectUrl(value) {
  try {
    const url = new URL(value);
    return url.hostname === 'social.dais.social' && url.pathname.startsWith('/users/social/');
  } catch {
    return false;
  }
}

async function resolveWebfingerActor(target) {
  const handle = target.trim().replace(/^@/, '');
  if (!handle.includes('@')) {
    throw new Error('target must be an actor URL or @user@domain handle');
  }
  const domain = handle.split('@').pop();
  publicHttpsUrl(`https://${domain}/`, 'target domain');
  const resource = `acct:${handle}`;
  const response = await fetch(`https://${domain}/.well-known/webfinger?resource=${encodeURIComponent(resource)}`, {
    headers: { Accept: 'application/jrd+json, application/json', 'User-Agent': 'dais-owner-api/1.0' },
  });
  if (!response.ok) {
    throw new Error(`could not resolve ${target}: HTTP ${response.status}`);
  }
  const jrd = await response.json();
  const link = (jrd.links || []).find((item) =>
    item.rel === 'self' && String(item.type || '').includes('activity+json') && item.href
  );
  if (!link) throw new Error(`no ActivityPub self link found for ${target}`);
  return publicHttpsUrl(link.href, 'actor link').toString();
}

function publicHttpsUrl(value, field) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${field} must be a valid URL`);
  }
  if (url.protocol !== 'https:') {
    throw new Error(`${field} must use https`);
  }
  const host = url.hostname.toLowerCase();
  if (
    host === 'localhost' ||
    host.endsWith('.local') ||
    host === '127.0.0.1' ||
    host === '::1' ||
    host.startsWith('10.') ||
    host.startsWith('192.168.') ||
    host.startsWith('169.254.') ||
    /^172\.(1[6-9]|2\d|3[0-1])\./.test(host)
  ) {
    throw new Error(`${field} host is not allowed`);
  }
  return url;
}

function normalizeHost(value) {
  const raw = optionalString(value);
  if (!raw) throw new Error('host is required');
  const host = raw
    .replace(/^https?:\/\//i, '')
    .split('/')[0]
    .split(':')[0]
    .trim()
    .toLowerCase();
  if (!/^[a-z0-9.-]+$/.test(host) || !host.includes('.')) {
    throw new Error('host must be a domain name');
  }
  if (
    host === 'localhost' ||
    host.endsWith('.local') ||
    host === '127.0.0.1' ||
    host.startsWith('10.') ||
    host.startsWith('192.168.') ||
    host.startsWith('169.254.') ||
    /^172\.(1[6-9]|2\d|3[0-1])\./.test(host)
  ) {
    throw new Error('host is not allowed');
  }
  return host;
}

function fixtureActivityPubActor(url) {
  const publicKeyPem = decodeFixturePublicKey(url.searchParams.get('pk') || '');
  if (!publicKeyPem) {
    return new Response('Missing or invalid fixture public key', { status: 400 });
  }
  const actorUrl = url.toString();
  const name = optionalString(url.searchParams.get('name')) || 'dais-s2s-fixture';
  const actor = {
    '@context': 'https://www.w3.org/ns/activitystreams',
    id: actorUrl,
    type: 'Application',
    preferredUsername: name,
    inbox: `${url.origin}/__dais-fixtures/activitypub/inbox`,
    publicKey: {
      id: `${actorUrl}#main-key`,
      owner: actorUrl,
      publicKeyPem,
    },
  };
  return new Response(JSON.stringify(actor), {
    headers: {
      'Content-Type': 'application/activity+json; charset=utf-8',
      'Cache-Control': 'no-store',
    },
  });
}

function decodeFixturePublicKey(value) {
  if (!value || value.length > 2000 || !/^[A-Za-z0-9_-]+$/.test(value)) return null;
  try {
    const base64 = value.replace(/-/g, '+').replace(/_/g, '/');
    const pem = atob(base64.padEnd(Math.ceil(base64.length / 4) * 4, '='));
    if (!pem.includes('-----BEGIN PUBLIC KEY-----') || !pem.includes('-----END PUBLIC KEY-----')) {
      return null;
    }
    return pem;
  } catch {
    return null;
  }
}

async function sourceId(sourceType, sourceUrl) {
  const bytes = new TextEncoder().encode(`${sourceType}\n${sourceUrl}`);
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const hex = [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `source-${hex.slice(0, 24)}`;
}

async function ownerNotifications(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, type, actor_id, actor_username, actor_display_name, actor_avatar_url,
            post_id, activity_id, content, read, created_at
     FROM notifications
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerDeliveries(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, post_id, target_type, target_url, protocol, status, retry_count,
            last_attempt_at, error_message, activity_type, created_at, delivered_at
     FROM deliveries
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerSourceSubscriptions(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
            last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
     FROM source_subscriptions
     ORDER BY updated_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerAddSource(env, body) {
  const sourceType = String(body.source_type || body.sourceType || '').trim().toLowerCase();
  if (!['rss', 'atom', 'api'].includes(sourceType)) {
    throw new Error('source_type must be rss, atom, or api');
  }
  const sourceUrl = publicHttpsUrl(body.url, 'source url').toString();
  const id = await sourceId(sourceType, sourceUrl);
  const title = optionalString(body.title);
  const cadenceMinutes = Math.max(5, Math.min(1440, Number(body.cadence_minutes || body.cadenceMinutes || 60)));
  const apiSecretName = optionalString(body.api_secret_name || body.apiSecretName);
  const policy = {
    private_reader_only: body.private_reader_only !== false && body.privateReaderOnly !== false,
    excerpt_only: body.excerpt_only !== false && body.excerptOnly !== false,
    link_required: body.link_required !== false && body.linkRequired !== false,
    attribution_required: body.attribution_required !== false && body.attributionRequired !== false,
    image_allowed: Boolean(body.image_allowed || body.imageAllowed),
    full_text_allowed: Boolean(body.full_text_allowed || body.fullTextAllowed),
  };
  await env.DB.prepare(`
    INSERT INTO source_subscriptions (
      id, source_type, url, title, refresh_cadence_minutes, policy_json,
      api_secret_name, status, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
    ON CONFLICT(id) DO UPDATE SET
      source_type = excluded.source_type,
      url = excluded.url,
      title = excluded.title,
      refresh_cadence_minutes = excluded.refresh_cadence_minutes,
      policy_json = excluded.policy_json,
      api_secret_name = excluded.api_secret_name,
      status = 'active',
      last_error = NULL,
      updated_at = CURRENT_TIMESTAMP
  `).bind(id, sourceType, sourceUrl, title, cadenceMinutes, JSON.stringify(policy), apiSecretName).run();
  return (await ownerSourceById(env, id)) || { id, source_type: sourceType, url: sourceUrl };
}

async function ownerRefreshSources(env, id) {
  const sources = id ? [await ownerSourceById(env, id)] : await ownerActiveSources(env);
  const rows = sources.filter(Boolean);
  if (id && rows.length === 0) throw new Error(`source not found: ${id}`);
  const results = [];
  for (const source of rows) {
    try {
      await refreshFeedSource(env, source, { manual: true });
      const refreshed = await ownerSourceById(env, source.id);
      results.push({ id: source.id, ok: true, status: refreshed?.status || 'active' });
    } catch (error) {
      const message = String(error && error.message ? error.message : error).slice(0, 500);
      await env.DB.prepare(`
        UPDATE source_subscriptions
        SET status = 'error',
            last_error = ?,
            error_count = error_count + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
      `).bind(message, source.id).run();
      results.push({ id: source.id, ok: false, error: message });
    }
  }
  return { ok: results.every((row) => row.ok), items: results };
}

async function ownerSourceById(env, id) {
  return env.DB.prepare(
    `SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
            etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
            policy_json, api_secret_name, created_at, updated_at
     FROM source_subscriptions
     WHERE id = ?1`,
  ).bind(id).first();
}

async function ownerActiveSources(env) {
  const rows = await env.DB.prepare(
    `SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
            etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
            policy_json, api_secret_name, created_at, updated_at
     FROM source_subscriptions
     WHERE status = 'active'
       AND source_type IN ('rss', 'atom', 'api')
     ORDER BY COALESCE(next_fetch_at, created_at) ASC
     LIMIT 20`,
  ).all();
  return rows.results || [];
}

async function ownerSourceItems(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, source_id, source_type, title, canonical_url, external_id, author,
            published_at, fetched_at, excerpt, content_type, thumbnail_url,
            rights_policy_json, read, summary, created_at, updated_at
     FROM source_items
     ORDER BY COALESCE(published_at, fetched_at) DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map((row) => ({
    id: row.id,
    title: row.title,
    source_type: row.source_type,
    canonical_url: row.canonical_url || null,
    excerpt: row.excerpt || row.summary || null,
    rights_policy_json: row.rights_policy_json || '{}',
    read: Boolean(row.read),
    source_id: row.source_id,
    author: row.author || null,
    published_at: row.published_at || null,
    fetched_at: row.fetched_at || null,
    thumbnail_url: row.thumbnail_url || null,
  }));
}

async function ownerModeration(env) {
  const settings = await ownerSettings(env);
  const [blocks, allowlist, blockRows, allowlistRows] = await Promise.all([
    env.DB.prepare("SELECT COUNT(*) AS count FROM blocks").first(),
    env.DB.prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1").first(),
    ownerBlocks(env),
    ownerAllowlist(env),
  ]);
  return {
    closed_network: Boolean(settings.closed_network),
    block_count: Number(blocks?.count || 0),
    allowlist_count: Number(allowlist?.count || 0),
    require_authorized_fetch: Boolean(settings.require_authorized_fetch),
    manually_approves_followers: Boolean(settings.manually_approves_followers),
    blocks: blockRows,
    allowlist: allowlistRows,
  };
}

async function ownerBlocks(env) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, blocked_domain, reason, created_at
     FROM blocks
     ORDER BY created_at DESC
     LIMIT 80`,
  ).all();
  return rows.results || [];
}

async function ownerAllowlist(env) {
  const rows = await env.DB.prepare(
    `SELECT host, note, enabled, created_at, updated_at
     FROM federation_allowlist
     ORDER BY host ASC
     LIMIT 120`,
  ).all();
  return rows.results || [];
}

async function ownerBlock(env, body) {
  const reason = optionalString(body.reason);
  const actorId = optionalString(body.actor_id || body.actorId);
  const domain = optionalString(body.domain || body.blocked_domain || body.blockedDomain);
  if (actorId) {
    const actorUrl = publicHttpsUrl(actorId, 'actor_id').toString();
    const id = `block-${stableId(actorUrl)}`;
    await env.DB.prepare(`
      INSERT OR REPLACE INTO blocks (id, actor_id, blocked_domain, reason, created_at)
      VALUES (?1, ?2, NULL, ?3, CURRENT_TIMESTAMP)
    `).bind(id, actorUrl, reason).run();
    return { ok: true, id, actor_id: actorUrl };
  }
  if (domain) {
    const host = normalizeHost(domain);
    const id = `block-domain-${host}`;
    await env.DB.prepare(`
      INSERT OR REPLACE INTO blocks (id, actor_id, blocked_domain, reason, created_at)
      VALUES (?1, ?2, ?2, ?3, CURRENT_TIMESTAMP)
    `).bind(id, host, reason).run();
    return { ok: true, id, blocked_domain: host };
  }
  throw new Error('actor_id or domain is required');
}

async function ownerAllowHost(env, body) {
  const host = normalizeHost(body.host);
  const note = optionalString(body.note);
  await env.DB.prepare(`
    INSERT INTO federation_allowlist (host, note, enabled, created_at, updated_at)
    VALUES (?1, ?2, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
    ON CONFLICT(host) DO UPDATE SET
      note = excluded.note,
      enabled = 1,
      updated_at = CURRENT_TIMESTAMP
  `).bind(host, note).run();
  return { ok: true, host };
}

async function ownerDiagnostics(env) {
  const [settings, posts, followers, deliveries] = await Promise.all([
    ownerSettings(env),
    env.DB.prepare("SELECT COUNT(*) AS count FROM posts").first(),
    env.DB.prepare("SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'").first(),
    env.DB.prepare("SELECT status, COUNT(*) AS count FROM deliveries GROUP BY status").all(),
  ]);
  const deliveryCounts = Object.fromEntries((deliveries.results || []).map((row) => [row.status, Number(row.count || 0)]));
  return [
    {
      key: 'owner-api',
      ok: true,
      detail: 'Authenticated owner API is available.',
    },
    {
      key: 'private-default',
      ok: settings.default_visibility === 'followers',
      detail: `default visibility is ${settings.default_visibility}`,
    },
    {
      key: 'activitypub',
      ok: true,
      detail: `posts=${Number(posts?.count || 0)} approved_followers=${Number(followers?.count || 0)}`,
    },
    {
      key: 'deliveries',
      ok: !deliveryCounts.failed,
      detail: Object.entries(deliveryCounts).map(([status, count]) => `${status}=${count}`).join(' ') || 'no deliveries',
    },
  ];
}

async function ownerCreatePost(env, {
  text,
  visibility,
  protocol,
  recipients = [],
  attachments = [],
  encrypt = false,
  inReplyTo = null,
  objectType = 'Note',
  pollOptions = null,
  summary = null,
}) {
  const actor = await env.DB.prepare(
    "SELECT id FROM actors WHERE username = 'social' LIMIT 1",
  ).first();
  const actorId = actor?.id || 'https://social.dais.social/users/social';
  const directTargets = visibility === 'direct'
    ? await ownerDirectDeliveryTargets(env, recipients)
    : [];
  const now = new Date().toISOString();
  const localId = `${now.replace(/[-:TZ.]/g, '').slice(0, 14)}-${stableId(`${now}\n${text}`).slice(0, 8)}`;
  const postId = `${actorId}/posts/${localId}`;
  const contentHtml = `<p>${escapeHtml(text).replaceAll('\n', '<br>')}</p>`;
  const mediaAttachments = normalizeAttachments(attachments);
  if (mediaAttachments.length > 0 && protocol !== 'activitypub') {
    throw new Error('media attachments currently require ActivityPub routing; AT Protocol media upload is not implemented yet');
  }
  if (mediaAttachments.length > 0 && encrypt) {
    throw new Error('E2EE media attachments require encrypted media support and are not implemented yet');
  }
  if (mediaAttachments.length > 0 && ['followers', 'direct'].includes(visibility) && !mediaAttachments.every(isPrivateMediaAttachment)) {
    throw new Error('private and direct media attachments must use private media upload URLs');
  }
  const normalizedObjectType = objectType === 'Question' ? 'Question' : 'Note';
  const pollOptionsJson = pollOptions ? JSON.stringify(pollOptions) : null;
  let replyTargetInbox = null;
  if (inReplyTo) {
    publicHttpsUrl(inReplyTo, 'in_reply_to');
    if (!isLocalObjectUrl(inReplyTo)) {
      replyTargetInbox = await resolveActivityPubObjectInbox(inReplyTo);
    }
  }
  const mediaAttachmentsJson = mediaAttachments.length ? JSON.stringify(mediaAttachments) : null;
  await env.DB.prepare(
    `INSERT INTO posts (
      id, actor_id, content, content_html, object_type, summary, visibility, protocol,
      published_at, media_attachments, in_reply_to, poll_options, created_at, updated_at
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)`,
  ).bind(
    postId,
    actorId,
    text,
    contentHtml,
    normalizedObjectType,
    summary,
    visibility,
    protocol,
    now,
    mediaAttachmentsJson,
    inReplyTo,
    pollOptionsJson,
  ).run();
  const deliveryIds =
    protocol === 'activitypub' || protocol === 'both'
      ? await ownerCreatePostDeliveries(env, { postId, visibility, directTargets, extraTargets: replyTargetInbox ? [replyTargetInbox] : [] })
      : [];
  return {
    id: postId,
    actor_id: actorId,
    content: text,
    content_html: contentHtml,
    object_type: normalizedObjectType,
    summary,
    visibility,
    protocol,
    published_at: now,
    in_reply_to: inReplyTo,
    poll_options: pollOptionsJson,
    recipients,
    attachments: mediaAttachments,
    delivery_ids: deliveryIds,
  };
}

function normalizeAttachments(values) {
  const attachments = [];
  for (const value of values || []) {
    let attachment = value;
    if (typeof value === 'string' && value.trim().startsWith('{')) {
      try {
        attachment = JSON.parse(value);
      } catch {
        throw new Error('attachment JSON is invalid');
      }
    } else if (typeof value === 'string') {
      attachment = { type: 'Document', url: value.trim() };
    }
    if (!attachment || typeof attachment !== 'object') {
      throw new Error('attachment must be a URL or object');
    }
    const url = optionalUrl(attachment.url, 'attachment url');
    const mediaType = optionalString(attachment.mediaType);
    if (mediaType && !allowedMediaType(mediaType)) {
      throw new Error('unsupported attachment media type');
    }
    const normalized = {
      type: optionalString(attachment.type) || (mediaType && mediaType.startsWith('image/') ? 'Image' : 'Document'),
      url,
    };
    if (mediaType) normalized.mediaType = mediaType;
    const name = optionalString(attachment.name);
    if (name) normalized.name = name;
    attachments.push(normalized);
  }
  return attachments;
}

function isPrivateMediaAttachment(attachment) {
  try {
    const url = new URL(attachment.url);
    return url.hostname === 'social.dais.social' && url.pathname.startsWith('/media/_private/');
  } catch {
    return false;
  }
}

async function ownerCreatePostDeliveries(env, { postId, visibility, directTargets, extraTargets = [] }) {
  if (visibility === 'direct') {
    return insertDeliveryRows(env, postId, [...directTargets, ...extraTargets]);
  }

  const followerDeliveries = await ownerCreateFollowerDeliveries(env, postId);
  const extraDeliveries = await insertDeliveryRows(env, postId, extraTargets);
  return [...followerDeliveries, ...extraDeliveries];
}

async function ownerDirectDeliveryTargets(env, recipients) {
  const placeholders = recipients.map((_, index) => `?${index + 1}`).join(', ');
  const rows = await env.DB.prepare(
    `SELECT follower_actor_id, follower_inbox
     FROM followers
     WHERE status = 'approved'
       AND follower_actor_id IN (${placeholders})`,
  ).bind(...recipients).all();
  const followers = rows.results || [];
  const knownRecipients = new Set(followers.map((row) => row.follower_actor_id));
  const missing = recipients.filter((recipient) => !knownRecipients.has(recipient));
  if (missing.length > 0) {
    throw new Error(`direct recipients must be approved followers with known inboxes: ${missing.join(', ')}`);
  }

  return followers.map((row) => row.follower_inbox);
}

async function ownerCreateFollowerDeliveries(env, postId) {
  const rows = await env.DB.prepare(
    `SELECT COALESCE(NULLIF(follower_shared_inbox, ''), follower_inbox) AS inbox
     FROM followers
     WHERE status = 'approved'`,
  ).all();
  return insertDeliveryRows(env, postId, (rows.results || []).map((row) => row.inbox));
}

async function insertDeliveryRows(env, postId, inboxes, activityType = 'Create', activityJson = null) {
  const allowedInboxes = [];
  for (const inbox of inboxes) {
    if (await ownerFederationTargetAllowed(env, inbox)) {
      allowedInboxes.push(inbox);
    }
  }
  const uniqueInboxes = [...new Set(allowedInboxes.map((value) => String(value || '').trim()).filter(Boolean))];
  const deliveryIds = [];
  const createdAt = new Date().toISOString();
  for (const inbox of uniqueInboxes) {
    const deliveryId = `delivery-${stableId(`${postId}\n${inbox}\n${createdAt}`).slice(0, 24)}`;
    await env.DB.prepare(
      `INSERT INTO deliveries (
        id, post_id, target_type, target_url, protocol,
        status, retry_count, created_at, activity_type, activity_json
      ) VALUES (
        ?1, ?2, 'inbox', ?3, 'activitypub',
        'queued', 0, ?4, ?5, ?6
      )`,
    ).bind(deliveryId, postId, inbox, createdAt, activityType, activityJson).run();
    deliveryIds.push(deliveryId);
  }
  return deliveryIds;
}

async function ownerFederationTargetAllowed(env, targetUrl) {
  const settings = await ownerSettings(env);
  if (!settings.closed_network) return true;
  let host = '';
  try {
    host = new URL(targetUrl).hostname.toLowerCase();
  } catch {
    return false;
  }
  const row = await env.DB.prepare(
    `SELECT 1
     FROM federation_allowlist
     WHERE host = ?1 AND enabled = 1
     LIMIT 1`,
  ).bind(host).first();
  return Boolean(row);
}

function requireOwnerBearer(request, env) {
  const configured = configuredOwnerToken(env);
  const isProduction = env.ENVIRONMENT === 'production';
  if (!configured && isProduction) {
    return apiJson({ error: 'OWNER_API_TOKEN is not configured' }, 503);
  }
  const expected = configured || 'dais-local-owner-token';
  const auth = request.headers.get('Authorization') || '';
  const provided = auth.startsWith('Bearer ') ? auth.slice(7).trim() : '';
  if (!provided || provided !== expected) {
    return apiJson({ error: 'Owner bearer token required' }, 401);
  }
  return null;
}

function configuredOwnerToken(env) {
  return env.OWNER_API_TOKEN || env.DAIS_OWNER_TOKEN || '';
}

async function readJson(request) {
  try {
    return await request.json();
  } catch {
    return {};
  }
}

function normalizeVisibility(value) {
  const normalized = String(value).toLowerCase();
  return ['public', 'unlisted', 'followers', 'direct'].includes(normalized) ? normalized : null;
}

function normalizeProtocol(value) {
  const normalized = String(value).toLowerCase().replace('_', '').replace('-', '');
  if (normalized === 'activitypub') return 'activitypub';
  if (normalized === 'atproto') return 'atproto';
  if (normalized === 'both') return 'both';
  return null;
}

function optionalString(value) {
  const trimmed = String(value || '').trim();
  return trimmed ? trimmed : null;
}

function optionalUrl(value, field) {
  const trimmed = optionalString(value);
  if (!trimmed) return null;
  let url;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error(`${field} must be an absolute https URL`);
  }
  if (url.protocol !== 'https:') {
    throw new Error(`${field} must be an absolute https URL`);
  }
  return trimmed;
}

function titleVisibility(value) {
  if (value === 'public') return 'Public';
  if (value === 'unlisted') return 'Unlisted';
  if (value === 'direct') return 'Direct';
  return 'Followers';
}

function titleProtocol(value) {
  if (value === 'atproto') return 'AtProto';
  if (value === 'both') return 'Both';
  return 'ActivityPub';
}

async function publicStatusCount(env) {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM posts WHERE visibility = 'public' AND encrypted_message IS NULL AND content NOT LIKE '%End-to-end encrypted message%'",
  ).first();
  return Number(row?.count || 0);
}

async function mastodonAccount(env) {
  const actor = await env.DB.prepare(
    "SELECT id, username, display_name, summary, avatar_url, header_url, created_at FROM actors WHERE username = 'social' LIMIT 1",
  ).first();
  const followers = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'",
  ).first();
  const following = await env.DB.prepare(
    "SELECT COUNT(*) AS count FROM following WHERE status = 'accepted'",
  ).first();
  const username = actor?.username || 'social';
  return {
    id: actor?.id || `https://social.dais.social/users/${username}`,
    username,
    acct: username,
    display_name: actor?.display_name || username,
    locked: true,
    bot: false,
    discoverable: false,
    group: false,
    created_at: actor?.created_at || new Date(0).toISOString(),
    note: actor?.summary || '',
    url: `https://social.dais.social/users/${username}`,
    avatar: actor?.avatar_url || '',
    avatar_static: actor?.avatar_url || '',
    header: actor?.header_url || '',
    header_static: actor?.header_url || '',
    followers_count: Number(followers?.count || 0),
    following_count: Number(following?.count || 0),
    statuses_count: await publicStatusCount(env),
    fields: [],
    emojis: [],
  };
}

async function mastodonStatuses(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
     FROM posts
     WHERE visibility = 'public'
       AND encrypted_message IS NULL
       AND content NOT LIKE '%End-to-end encrypted message%'
     ORDER BY published_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  const account = await mastodonAccount(env);
  return (rows.results || []).map((row) => statusJson(row, account));
}

async function mastodonStatus(env, id) {
  const row = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
     FROM posts
     WHERE id = ?1
       AND visibility = 'public'
       AND encrypted_message IS NULL
       AND content NOT LIKE '%End-to-end encrypted message%'
     LIMIT 1`,
  ).bind(id).first();
  if (!row) return null;
  return statusJson(row, await mastodonAccount(env));
}

async function mastodonStatusContext(env, id) {
  const status = await mastodonStatus(env, id);
  if (!status) return null;

  const ancestors = [];
  const seenAncestors = new Set([id]);
  let parentId = status.in_reply_to_id;
  while (parentId && ancestors.length < 20 && !seenAncestors.has(parentId)) {
    seenAncestors.add(parentId);
    const parent = await mastodonStatus(env, parentId);
    if (!parent) break;
    ancestors.unshift(parent);
    parentId = parent.in_reply_to_id;
  }

  const account = await mastodonAccount(env);
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
     FROM posts
     WHERE in_reply_to = ?1
       AND visibility = 'public'
       AND encrypted_message IS NULL
       AND content NOT LIKE '%End-to-end encrypted message%'
     ORDER BY published_at ASC
     LIMIT 40`,
  ).bind(id).all();

  return {
    ancestors,
    descendants: (rows.results || []).map((row) => statusJson(row, account)),
  };
}

function statusJson(row, account) {
  return {
    id: row.id,
    uri: row.id,
    url: row.id,
    account,
    in_reply_to_id: row.in_reply_to || null,
    in_reply_to_account_id: null,
    reblog: null,
    content: mastodonStatusContent(row),
    plain_text: mastodonPlainText(row),
    created_at: row.published_at,
    edited_at: null,
    emojis: [],
    replies_count: Number(row.reply_count || 0),
    reblogs_count: Number(row.boost_count || 0),
    favourites_count: Number(row.like_count || 0),
    reblogged: Boolean(row.reblogged),
    favourited: Boolean(row.favourited),
    muted: false,
    sensitive: false,
    spoiler_text: '',
    visibility: mastodonVisibility(row.visibility),
    media_attachments: mastodonMediaAttachments(row),
    mentions: [],
    tags: [],
    card: null,
    poll: mastodonPollJson(row),
  };
}

function mastodonPollJson(row) {
  if (row.object_type !== 'Question' || !row.poll_options) return null;
  let parsed;
  try {
    parsed = typeof row.poll_options === 'string' ? JSON.parse(row.poll_options) : row.poll_options;
  } catch {
    return null;
  }
  const options = Array.isArray(parsed?.options) ? parsed.options.map((title) => String(title)) : [];
  if (options.length === 0) return null;
  return {
    id: `${row.id}#poll`,
    expires_at: null,
    expired: false,
    multiple: Boolean(parsed.multiple),
    votes_count: 0,
    voters_count: 0,
    voted: false,
    own_votes: [],
    options: options.map((title) => ({ title, votes_count: 0 })),
    emojis: [],
  };
}

function mastodonMediaAttachments(row) {
  const attachments = parseAttachmentArray(row.media_attachments);
  return attachments.map((attachment, index) => {
    const url = String(attachment.url || '');
    const mediaType = String(attachment.mediaType || '');
    return {
      id: `${row.id}#media-${index + 1}`,
      type: mediaType.startsWith('image/') ? 'image' : mediaType.startsWith('video/') ? 'video' : 'unknown',
      url,
      preview_url: url,
      remote_url: null,
      preview_remote_url: null,
      text_url: null,
      meta: {},
      description: attachment.name || null,
      blurhash: null,
    };
  });
}

function parseAttachmentArray(value) {
  if (!value) return [];
  try {
    const parsed = typeof value === 'string' ? JSON.parse(value) : value;
    return Array.isArray(parsed) ? parsed.filter((item) => item && item.url) : [];
  } catch {
    return [];
  }
}

function mastodonPlainText(row) {
  return [row.name, row.summary, row.content].filter(Boolean).join('\n\n');
}

function mastodonStatusContent(row) {
  const parts = [];
  if (row.name) parts.push(`<p><strong>${escapeHtml(row.name)}</strong></p>`);
  if (row.summary) parts.push(`<p>${escapeHtml(row.summary)}</p>`);
  parts.push(row.content_html || escapeHtml(row.content || ''));
  return parts.join('');
}

async function mastodonUpdateStatus(env, id, { text, summary }) {
  const existing = await mastodonStatus(env, id);
  if (!existing) return null;
  const assignments = ['updated_at = CURRENT_TIMESTAMP'];
  const values = [];
  if (text) {
    values.push(text);
    assignments.push(`content = ?${values.length}`);
    values.push(`<p>${escapeHtml(text).replaceAll('\n', '<br>')}</p>`);
    assignments.push(`content_html = ?${values.length}`);
  }
  values.push(summary);
  assignments.push(`summary = ?${values.length}`);
  values.push(id);
  await env.DB.prepare(
    `UPDATE posts SET ${assignments.join(', ')} WHERE id = ?${values.length}`,
  ).bind(...values).run();
  return mastodonStatus(env, id);
}

async function mastodonDeleteStatus(env, id) {
  const existing = await mastodonStatus(env, id);
  if (!existing) return null;
  const localActor = await ownerLocalActor(env);
  const now = new Date().toISOString();
  const deleteId = `${localActor.id}#deletes/${stableId(`${id}\n${now}`).slice(0, 16)}`;
  const activity = {
    '@context': 'https://www.w3.org/ns/activitystreams',
    id: deleteId,
    type: 'Delete',
    actor: localActor.id,
    published: now,
    to: ['https://www.w3.org/ns/activitystreams#Public'],
    cc: [`${localActor.id}/followers`],
    object: {
      id,
      type: 'Tombstone',
    },
  };
  const rows = await env.DB.prepare(
    `SELECT COALESCE(NULLIF(follower_shared_inbox, ''), follower_inbox) AS inbox
     FROM followers
     WHERE status = 'approved'`,
  ).all();
  await insertDeliveryRows(env, id, (rows.results || []).map((row) => row.inbox), 'Delete', JSON.stringify(activity));
  await env.DB.prepare("DELETE FROM posts WHERE id = ?1").bind(id).run();
  return existing;
}

async function mastodonToggleStatusInteraction(env, statusId, action) {
  const localActor = await ownerLocalActor(env);
  const type = action === 'favourite' || action === 'unfavourite' ? 'like' : 'boost';
  const interactionId = `${localActor.id}#${type}s/${stableId(statusId).slice(0, 16)}`;
  if (action === 'unfavourite' || action === 'unreblog') {
    await env.DB.prepare("DELETE FROM interactions WHERE id = ?1").bind(interactionId).run();
    return;
  }
  await env.DB.prepare(
    `INSERT OR REPLACE INTO interactions (
       id, type, actor_id, object_url, created_at
     ) VALUES (?1, ?2, ?3, ?4, ?5)`,
  ).bind(interactionId, type, localActor.id, statusId, new Date().toISOString()).run();
}

async function mastodonStatusesByInteraction(env, type, limit) {
  const rows = await env.DB.prepare(
    `SELECT p.id, p.actor_id, p.content, p.content_html, COALESCE(p.object_type, 'Note') AS object_type,
            p.name, p.summary, p.visibility, p.published_at, p.in_reply_to, p.poll_options, p.media_attachments,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = p.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions li WHERE (li.post_id = p.id OR li.object_url = p.id) AND li.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions bi WHERE (bi.post_id = p.id OR bi.object_url = p.id) AND bi.type = 'boost') AS boost_count,
            EXISTS(SELECT 1 FROM interactions oi WHERE (oi.post_id = p.id OR oi.object_url = p.id) AND oi.type = 'like' AND oi.actor_id = p.actor_id) AS favourited,
            EXISTS(SELECT 1 FROM interactions oi WHERE (oi.post_id = p.id OR oi.object_url = p.id) AND oi.type = 'boost' AND oi.actor_id = p.actor_id) AS reblogged
     FROM posts p
     JOIN interactions i ON i.object_url = p.id OR i.post_id = p.id
     WHERE i.type = ?1
       AND p.visibility = 'public'
       AND p.encrypted_message IS NULL
       AND p.content NOT LIKE '%End-to-end encrypted message%'
     ORDER BY i.created_at DESC
     LIMIT ?2`,
  ).bind(type, limit).all();
  const account = await mastodonAccount(env);
  return (rows.results || []).map((row) => statusJson(row, account));
}

async function mastodonRelationships(env, ids) {
  const uniqueIds = [...new Set((ids || []).map((id) => String(id || '').trim()).filter(Boolean))];
  const relationships = [];
  for (const id of uniqueIds) {
    const [following, followedBy, blocked] = await Promise.all([
      env.DB.prepare("SELECT status FROM following WHERE target_actor_id = ?1 LIMIT 1").bind(id).first(),
      env.DB.prepare("SELECT status FROM followers WHERE follower_actor_id = ?1 LIMIT 1").bind(id).first(),
      env.DB.prepare("SELECT 1 FROM blocks WHERE actor_id = ?1 OR blocked_domain = ?1 LIMIT 1").bind(id).first(),
    ]);
    relationships.push({
      id,
      following: following?.status === 'accepted',
      showing_reblogs: true,
      notifying: false,
      followed_by: followedBy?.status === 'approved',
      blocking: Boolean(blocked),
      blocked_by: false,
      muting: false,
      muting_notifications: false,
      requested: following?.status === 'pending',
      domain_blocking: false,
      endorsed: false,
      note: '',
    });
  }
  return relationships;
}

async function mastodonSetBlock(env, actorId, enabled) {
  if (enabled) {
    await env.DB.prepare(
      `INSERT OR REPLACE INTO blocks (id, actor_id, reason, created_at)
       VALUES (?1, ?2, 'Mastodon API block', CURRENT_TIMESTAMP)`,
    ).bind(`block-${stableId(actorId)}`, actorId).run();
  } else {
    await env.DB.prepare("DELETE FROM blocks WHERE actor_id = ?1").bind(actorId).run();
  }
}

async function mastodonBlocks(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT actor_id, actor_id AS url, created_at
     FROM blocks
     WHERE actor_id IS NOT NULL AND actor_id != ''
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map(remoteAccountJson);
}

async function mastodonSearch(env, query, limit) {
  const term = String(query || '').trim();
  if (!term) return { accounts: [], statuses: [], hashtags: [] };
  if (term.startsWith('@') || term.startsWith('https://')) {
    try {
      const actor = await ownerDiscoverActor(env, term);
      return {
        accounts: [remoteAccountJson({ actor_id: actor.id, url: actor.id, created_at: new Date(0).toISOString() })],
        statuses: [],
        hashtags: [],
      };
    } catch {
      // Fall through to status search.
    }
  }
  const rows = await env.DB.prepare(
    `SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
            name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
            (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
            (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
            EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
     FROM posts
     WHERE visibility = 'public'
       AND encrypted_message IS NULL
       AND content NOT LIKE '%End-to-end encrypted message%'
       AND (content LIKE ?1 OR name LIKE ?1 OR summary LIKE ?1)
     ORDER BY published_at DESC
     LIMIT ?2`,
  ).bind(`%${term}%`, limit).all();
  const account = await mastodonAccount(env);
  return {
    accounts: [],
    statuses: (rows.results || []).map((row) => statusJson(row, account)),
    hashtags: [],
  };
}

async function mastodonConversations(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, participants, last_message_at
     FROM conversations
     ORDER BY last_message_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map((row) => ({
    id: row.id,
    unread: false,
    last_status: null,
    accounts: parseJsonArray(row.participants).map((actorId) => remoteAccountJson({ actor_id: actorId, url: actorId })),
  }));
}

async function mastodonUploadMedia(request, env) {
  const body = await readRequestBody(request);
  let file = body.file || body['file[]'];
  let filename = String(body.filename || body.description || 'upload.bin');
  let mediaType = String(body.media_type || body.content_type || '');
  let dataBase64 = body.data_base64 || null;
  if (file && typeof file === 'object' && typeof file.arrayBuffer === 'function') {
    filename = file.name || filename;
    mediaType = file.type || mediaType || mediaTypeForFilename(filename);
    dataBase64 = bytesToBase64(new Uint8Array(await file.arrayBuffer()));
  }
  if (!dataBase64) throw new Error('file is required');
  const uploaded = await ownerUploadMedia(env, {
    filename,
    data_base64: dataBase64,
    media_type: mediaType || mediaTypeForFilename(filename),
    access: 'public',
  });
  return mastodonMediaAttachmentFromUpload(uploaded, body.description || null);
}

function mastodonMediaAttachmentFromUpload(uploaded, description = null) {
  const attachment = uploaded.attachment || {};
  const url = uploaded.url || attachment.url;
  const mediaType = uploaded.media_type || attachment.mediaType || '';
  return {
    id: url,
    type: mediaType.startsWith('image/') ? 'image' : mediaType.startsWith('video/') ? 'video' : 'unknown',
    url,
    preview_url: url,
    remote_url: null,
    preview_remote_url: null,
    text_url: null,
    meta: {},
    description: description || attachment.name || null,
    blurhash: null,
  };
}

async function mastodonAttachmentsForMediaIds(env, mediaIds, visibility) {
  return mediaIds.map((id) => {
    const url = String(id || '').trim();
    if (!url) return null;
    if (visibility === 'followers' || visibility === 'direct') {
      if (!isPrivateMediaAttachment({ url })) {
        throw new Error('Mastodon API private media posts require private media URLs');
      }
    }
    return {
      type: 'Document',
      url,
      mediaType: mediaTypeForFilename(url),
      name: decodeURIComponent(url.split('/').pop() || 'media'),
    };
  }).filter(Boolean);
}

function mastodonPollFromRequestBody(body) {
  const options = mastodonPollOptionsFromRequestBody(body);
  const multiple = mastodonPollMultipleFromRequestBody(body);
  if (options.length === 0) {
    if (multiple) throw new Error('poll[multiple] requires poll[options][]');
    return null;
  }
  if (options.length < 2 || options.length > 4) {
    throw new Error('polls require between two and four options');
  }
  for (const option of options) {
    if (!option.trim()) throw new Error('poll options must not be empty');
    if ([...option].length > 200) throw new Error('poll options must be 200 characters or fewer');
  }
  return { multiple, options };
}

function mastodonPollOptionsFromRequestBody(body) {
  const poll = body.poll && typeof body.poll === 'object' ? body.poll : {};
  const candidates = [
    poll.options,
    body['poll[options]'],
    body['poll[options][]'],
  ];
  for (const candidate of candidates) {
    const values = arrayFromBodyValue(candidate).map((value) => String(value).trim()).filter(Boolean);
    if (values.length > 0) return values;
  }
  return [];
}

function mastodonPollMultipleFromRequestBody(body) {
  const poll = body.poll && typeof body.poll === 'object' ? body.poll : {};
  const value = poll.multiple !== undefined ? poll.multiple : body['poll[multiple]'];
  return ['true', '1', 'on', 'yes'].includes(String(value || '').toLowerCase());
}

function requestBodyArray(body, key) {
  const bracket = `${key}[]`;
  const value = body[bracket] !== undefined ? body[bracket] : body[key];
  return arrayFromBodyValue(value).map(String).filter(Boolean);
}

function arrayFromBodyValue(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value === undefined || value === null || value === '') return [];
  return [String(value)];
}

function parseJsonArray(value) {
  try {
    const parsed = typeof value === 'string' ? JSON.parse(value) : value;
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function bytesToBase64(bytes) {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

async function mastodonNotifications(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, type, actor_id, actor_username, actor_display_name, content, post_id, created_at
     FROM notifications
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map((row) => ({
    id: row.id,
    type: mastodonNotificationType(row.type),
    created_at: row.created_at,
    account: {
      id: row.actor_id,
      username: row.actor_username || row.actor_id,
      acct: row.actor_username || row.actor_id,
      display_name: row.actor_display_name || row.actor_username || row.actor_id,
      url: row.actor_id,
      avatar: '',
      avatar_static: '',
      header: '',
      header_static: '',
      locked: false,
      bot: false,
      fields: [],
      emojis: [],
    },
    status: row.post_id ? { id: row.post_id, uri: row.post_id, url: row.post_id } : null,
  }));
}

async function mastodonFollowers(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT follower_actor_id AS actor_id, follower_actor_id AS url, status, created_at
     FROM followers
     WHERE status = 'approved'
     ORDER BY updated_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map(remoteAccountJson);
}

async function mastodonFollowing(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT target_actor_id AS actor_id, target_actor_id AS url, status, created_at
     FROM following
     WHERE status = 'accepted'
     ORDER BY created_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return (rows.results || []).map(remoteAccountJson);
}

function remoteAccountJson(row) {
  const url = row.url || row.actor_id || '';
  const parsed = parseActorAcct(url);
  return {
    id: url,
    username: parsed.username,
    acct: parsed.acct,
    display_name: parsed.username,
    locked: false,
    bot: false,
    discoverable: false,
    group: false,
    created_at: row.created_at || new Date(0).toISOString(),
    note: '',
    url,
    avatar: '',
    avatar_static: '',
    header: '',
    header_static: '',
    followers_count: 0,
    following_count: 0,
    statuses_count: 0,
    fields: [],
    emojis: [],
  };
}

function parseActorAcct(actorUrl) {
  try {
    const url = new URL(actorUrl);
    const username = decodeURIComponent(url.pathname.split('/').filter(Boolean).pop() || url.hostname);
    return { username, acct: `${username}@${url.hostname}` };
  } catch {
    return { username: actorUrl, acct: actorUrl };
  }
}

function mastodonNotificationType(value) {
  if (value === 'like') return 'favourite';
  if (value === 'boost') return 'reblog';
  return value || 'mention';
}

async function readRequestBody(request) {
  const contentType = request.headers.get('Content-Type') || '';
  if (contentType.includes('application/json')) return readJson(request);
  if (contentType.includes('application/x-www-form-urlencoded') || contentType.includes('multipart/form-data')) {
    const form = await request.formData();
    const body = {};
    for (const [key, value] of form.entries()) {
      if (body[key] === undefined) {
        body[key] = value;
      } else if (Array.isArray(body[key])) {
        body[key].push(value);
      } else {
        body[key] = [body[key], value];
      }
    }
    return body;
  }
  return {};
}

function normalizeMastodonVisibility(value) {
  const normalized = String(value).toLowerCase();
  if (normalized === 'public') return 'public';
  if (normalized === 'unlisted') return 'unlisted';
  if (normalized === 'private' || normalized === 'followers') return 'followers';
  if (normalized === 'direct') return 'direct';
  return null;
}

function mastodonVisibility(value) {
  if (value === 'public' || value === 'unlisted' || value === 'direct') return value;
  return 'private';
}

function requireBearer(request, env = {}) {
  const configured = configuredOwnerToken(env);
  const isProduction = env.ENVIRONMENT === 'production';
  if (!configured && isProduction) {
    return apiJson({ error: 'OWNER_API_TOKEN is not configured' }, 503);
  }
  const expected = configured || 'dais-local-owner-token';
  const auth = request.headers.get('Authorization') || '';
  const provided = auth.startsWith('Bearer ') ? auth.slice(7).trim() : '';
  if (provided && provided === expected) return null;
  return apiJson({ error: 'Bearer token required' }, 401);
}

function clampLimit(value) {
  const parsed = Number.parseInt(value || '20', 10);
  if (!Number.isFinite(parsed)) return 20;
  return Math.max(1, Math.min(parsed, 80));
}

function stableId(value) {
  let hash = 5381;
  for (const ch of String(value)) hash = ((hash << 5) + hash) ^ ch.charCodeAt(0);
  return Math.abs(hash >>> 0).toString(36);
}

function apiJson(value, status = 200) {
  return new Response(status === 204 ? null : JSON.stringify(value), {
    status,
    headers: {
      'Content-Type': 'application/json; charset=utf-8',
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Headers': 'Authorization, Content-Type',
      'Access-Control-Allow-Methods': 'GET, POST, PUT, PATCH, DELETE, OPTIONS',
    },
  });
}

function textHeaders(contentType) {
  return {
    'Content-Type': contentType,
    'Access-Control-Allow-Origin': '*',
  };
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}
