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

    if (path.startsWith('/api/v1/') || path.startsWith('/oauth/')) {
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

  if (request.method === 'GET' && path === '/api/v1/instance') {
    return apiJson({
      uri: 'social.dais.social',
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
    });
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

  const account = path.match(/^\/api\/v1\/accounts\/([^/]+)$/);
  if (request.method === 'GET' && account) {
    return apiJson(await mastodonAccount(env));
  }

  const status = path.match(/^\/api\/v1\/statuses\/(.+)$/);
  if (request.method === 'GET' && status) {
    const value = await mastodonStatus(env, decodeURIComponent(status[1]));
    if (!value) return apiJson({ error: 'Record not found' }, 404);
    return apiJson(value);
  }

  if (request.method === 'GET' && path === '/api/v1/notifications') {
    const auth = requireBearer(request);
    if (auth) return auth;
    return apiJson(await mastodonNotifications(env, clampLimit(url.searchParams.get('limit'))));
  }

  return apiJson({ error: 'Not implemented in read-only dais Mastodon API floor' }, 404);
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
    visibility: 'public',
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

function mastodonNotificationType(value) {
  if (value === 'like') return 'favourite';
  if (value === 'boost') return 'reblog';
  return value || 'mention';
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
