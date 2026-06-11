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

    if (path.startsWith('/api/dais/owner/')) {
      return handleOwnerApi(request, env, url);
    }

    if (path.startsWith('/api/v1/') || path.startsWith('/api/v2/') || path.startsWith('/oauth/')) {
      return handleMastodonApi(request, env, url);
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
  // Extract filename from /media/filename.ext
  const filename = path.substring(7); // Remove '/media/'

  if (!filename) {
    return new Response('Missing filename', { status: 400 });
  }

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
    headers.set('Cache-Control', 'public, max-age=31536000, immutable');
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
          media_attachments: { supported_mime_types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
          polls: { max_options: 0, max_characters_per_option: 0, min_expiration: 0, max_expiration: 0 },
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
    return apiJson({
      access_token: 'dais-local-owner-token',
      token_type: 'Bearer',
      scope: 'read',
      created_at: Math.floor(Date.now() / 1000),
    });
  }

  if (request.method === 'POST' && path === '/oauth/revoke') {
    return apiJson({});
  }

  if (request.method === 'GET' && path === '/api/v1/preferences') {
    const auth = requireBearer(request);
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

  if (request.method === 'GET' && path === '/api/v1/accounts/verify_credentials') {
    const auth = requireBearer(request);
    if (auth) return auth;
    return apiJson(await mastodonAccount(env));
  }

  if (request.method === 'GET' && path === '/api/v1/timelines/public') {
    return apiJson(await mastodonStatuses(env, clampLimit(url.searchParams.get('limit'))));
  }

  if (request.method === 'GET' && path === '/api/v1/timelines/home') {
    const auth = requireBearer(request);
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

  const statusContext = path.match(/^\/api\/v1\/statuses\/(.+)\/context$/);
  if (request.method === 'GET' && statusContext) {
    const value = await mastodonStatus(env, decodeURIComponent(statusContext[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson({ ancestors: [], descendants: [] });
  }

  const statusAction = path.match(/^\/api\/v1\/statuses\/(.+)\/(favourite|unfavourite|reblog|unreblog)$/);
  if (request.method === 'POST' && statusAction) {
    const auth = requireBearer(request);
    if (auth) return auth;
    const value = await mastodonStatus(env, decodeURIComponent(statusAction[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(value);
  }

  const status = path.match(/^\/api\/v1\/statuses\/([^/]+)$/);
  if (request.method === 'GET' && status) {
    const value = await mastodonStatus(env, decodeURIComponent(status[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(value);
  }

  if (request.method === 'POST' && path === '/api/v1/statuses') {
    const auth = requireBearer(request);
    if (auth) return auth;
    const body = await readRequestBody(request);
    const text = String(body.status || body.text || '').trim();
    if (!text) return apiJson({ error: 'status is required' }, 400);
    const visibility = normalizeMastodonVisibility(body.visibility || 'private') || 'followers';
    const created = await ownerCreatePost(env, {
      text,
      visibility,
      protocol: 'activitypub',
    });
    return apiJson(statusJson({
      id: created.id,
      actor_id: created.actor_id,
      content: created.content,
      content_html: created.content_html,
      object_type: 'Note',
      name: null,
      summary: body.spoiler_text || null,
      visibility: created.visibility,
      published_at: created.published_at,
      in_reply_to: body.in_reply_to_id || null,
    }, await mastodonAccount(env)), 201);
  }

  if (request.method === 'GET' && path === '/api/v1/notifications') {
    const auth = requireBearer(request);
    if (auth) return auth;
    return apiJson(await mastodonNotifications(env, clampLimit(url.searchParams.get('limit'))));
  }

  return apiJson({ error: 'Not implemented in read-only dais Mastodon API floor' }, 404);
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

  if (request.method === 'GET' && path === '/posts') {
    return apiJson({
      items: await ownerPosts(env, clampLimit(url.searchParams.get('limit'))),
    });
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

  if (request.method === 'GET' && path === '/following') {
    return apiJson({
      items: await ownerFollowing(env, clampLimit(url.searchParams.get('limit'))),
    });
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

  if (request.method === 'GET' && path === '/moderation') {
    return apiJson(await ownerModeration(env));
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

    const created = await ownerCreatePost(env, { text, visibility, protocol });
    return apiJson(created, 201);
  }

  return apiJson({ error: 'Not implemented in dais owner API' }, 404);
}

async function ownerSnapshot(env) {
  const [posts, sources, moderation, diagnostics] = await Promise.all([
    ownerPosts(env, 20),
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
    posts: posts.map((post) => ({
      id: post.id,
      title: post.name || null,
      content: post.content || '',
      visibility: titleVisibility(post.visibility),
      protocol: titleProtocol(post.protocol),
      encrypted: Boolean(post.encrypted_message),
      published_at: post.published_at || null,
    })),
    sources,
    moderation,
    diagnostics,
  };
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
            atproto_uri, atproto_cid, encrypted_message, published_at, created_at, updated_at
     FROM posts
     ORDER BY published_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
}

async function ownerHomeTimeline(env, limit) {
  const rows = await env.DB.prepare(
    `SELECT id, object_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
            content, content_html, visibility, in_reply_to, published_at, updated_at,
            protocol, created_at
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
     ORDER BY updated_at DESC
     LIMIT ?1`,
  ).bind(limit).all();
  return rows.results || [];
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
  const [blocks, allowlist] = await Promise.all([
    env.DB.prepare("SELECT COUNT(*) AS count FROM blocks").first(),
    env.DB.prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1").first(),
  ]);
  return {
    closed_network: Boolean(settings.closed_network),
    block_count: Number(blocks?.count || 0),
    allowlist_count: Number(allowlist?.count || 0),
    require_authorized_fetch: Boolean(settings.require_authorized_fetch),
    manually_approves_followers: Boolean(settings.manually_approves_followers),
  };
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

async function ownerCreatePost(env, { text, visibility, protocol }) {
  const actor = await env.DB.prepare(
    "SELECT id FROM actors WHERE username = 'social' LIMIT 1",
  ).first();
  const actorId = actor?.id || 'https://social.dais.social/users/social';
  const now = new Date().toISOString();
  const localId = `${now.replace(/[-:TZ.]/g, '').slice(0, 14)}-${stableId(`${now}\n${text}`).slice(0, 8)}`;
  const postId = `${actorId}/posts/${localId}`;
  const contentHtml = `<p>${escapeHtml(text).replaceAll('\n', '<br>')}</p>`;
  await env.DB.prepare(
    `INSERT INTO posts (
      id, actor_id, content, content_html, object_type, visibility, protocol, published_at, created_at, updated_at
    ) VALUES (?1, ?2, ?3, ?4, 'Note', ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)`,
  ).bind(postId, actorId, text, contentHtml, visibility, protocol, now).run();
  return {
    id: postId,
    actor_id: actorId,
    content: text,
    content_html: contentHtml,
    visibility,
    protocol,
    published_at: now,
    delivery_ids: [],
  };
}

function requireOwnerBearer(request, env) {
  const configured = env.OWNER_API_TOKEN || env.DAIS_OWNER_TOKEN || '';
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
            name, summary, visibility, published_at, in_reply_to
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
            name, summary, visibility, published_at, in_reply_to
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
    replies_count: 0,
    reblogs_count: 0,
    favourites_count: 0,
    reblogged: false,
    favourited: false,
    muted: false,
    sensitive: false,
    spoiler_text: '',
    visibility: mastodonVisibility(row.visibility),
    media_attachments: [],
    mentions: [],
    tags: [],
    card: null,
    poll: null,
  };
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
    return Object.fromEntries(form.entries());
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

function requireBearer(request) {
  const auth = request.headers.get('Authorization') || '';
  if (auth.startsWith('Bearer ') && auth.slice(7).trim()) return null;
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
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
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
