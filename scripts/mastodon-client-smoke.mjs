#!/usr/bin/env node

const config = {
  baseUrl: process.env.DAIS_SOCIAL_BASE_URL || "https://social.dais.social",
  token: process.env.DAIS_MASTODON_BEARER_TOKEN || "",
};

const tinyPngBytes = Uint8Array.from(
  Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
    "base64",
  ),
);

const createdIds = [];

async function request(path, options = {}) {
  const headers = new Headers(options.headers || {});
  if (options.auth) {
    if (!config.token) throw new Error("set DAIS_MASTODON_BEARER_TOKEN for authenticated client smoke");
    headers.set("Authorization", `Bearer ${config.token}`);
  }
  const response = await fetch(`${config.baseUrl}${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body,
    redirect: options.redirect || "follow",
  });
  const text = await response.text();
  let json;
  try {
    json = text ? JSON.parse(text) : undefined;
  } catch {
    json = undefined;
  }
  return {
    status: response.status,
    location: response.headers.get("location") || "",
    contentType: response.headers.get("content-type") || "",
    text,
    json,
  };
}

function form(values) {
  const body = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) {
    if (Array.isArray(value)) {
      for (const item of value) body.append(key, item);
    } else if (value !== undefined && value !== null) {
      body.set(key, value);
    }
  }
  return body;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function cleanup() {
  for (const id of createdIds.filter(Boolean).reverse()) {
    await request(`/api/v1/statuses/${encodeURIComponent(id)}`, {
      method: "DELETE",
      auth: true,
    }).catch(() => {});
  }
}

async function main() {
  try {
    const app = await request("/api/v1/apps", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: form({
        client_name: "dais Mastodon client smoke",
        redirect_uris: "urn:ietf:wg:oauth:2.0:oob",
        scopes: "read write follow",
        website: "https://github.com/marctjones/dais",
      }),
    });
    assert(app.status === 200, `app registration expected 200, got ${app.status}: ${app.text}`);
    assert(app.json?.client_id && app.json?.client_secret, "app registration did not return client credentials");
    assert(app.json.name === "dais Mastodon client smoke", "form app registration ignored client_name");

    const authorize = await request(
      `/oauth/authorize?${form({
        response_type: "code",
        client_id: app.json.client_id,
        redirect_uri: "urn:ietf:wg:oauth:2.0:oob",
        scope: "read write follow",
        state: "dais-client-smoke",
      })}`,
    );
    assert(authorize.status === 200, `authorize expected 200, got ${authorize.status}`);
    assert(authorize.text.includes("dais-local-owner"), "authorize response did not expose OOB code");

    const token = await request("/oauth/token", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: form({
        grant_type: "authorization_code",
        code: "dais-local-owner",
        client_id: app.json.client_id,
        client_secret: app.json.client_secret,
        redirect_uri: "urn:ietf:wg:oauth:2.0:oob",
      }),
    });
    assert(token.status === 200, `token expected 200, got ${token.status}: ${token.text}`);
    assert(token.json?.access_token === "owner-token-required", "token endpoint leaked or changed placeholder token");
    assert(token.json?.dais_owner_token_required === true, "token endpoint did not mark owner-token requirement");

    const placeholderAuth = await request("/api/v1/accounts/verify_credentials", {
      headers: { Authorization: "Bearer owner-token-required" },
    });
    assert(placeholderAuth.status === 401, "placeholder token unexpectedly authenticated");

    const account = await request("/api/v1/accounts/verify_credentials", { auth: true });
    assert(account.status === 200, `verify_credentials expected 200, got ${account.status}`);
    assert(account.json?.id, "verify_credentials account shape incomplete");

    const poll = await request("/api/v1/statuses", {
      method: "POST",
      auth: true,
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: form({
        status: `dais Mastodon client smoke poll ${new Date().toISOString()}`,
        visibility: "public",
        "poll[options][]": ["CLI", "TUI"],
        "poll[multiple]": "false",
        "poll[expires_in]": "300",
      }),
    });
    assert(poll.status === 201, `poll create expected 201, got ${poll.status}: ${poll.text}`);
    createdIds.push(poll.json?.id);
    assert(poll.json?.poll?.options?.length === 2, "form poll options did not round-trip");

    const mediaForm = new FormData();
    mediaForm.set("description", "dais Mastodon client smoke pixel");
    mediaForm.set("file", new Blob([tinyPngBytes], { type: "image/png" }), "dais-client-smoke.png");
    const media = await request("/api/v1/media", {
      method: "POST",
      auth: true,
      body: mediaForm,
    });
    assert(media.status === 200, `multipart media upload expected 200, got ${media.status}: ${media.text}`);
    assert(media.json?.id && media.json?.type === "image", "multipart media upload shape incomplete");

    const mediaPost = await request("/api/v1/statuses", {
      method: "POST",
      auth: true,
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: form({
        status: `dais Mastodon client smoke media ${new Date().toISOString()}`,
        visibility: "public",
        "media_ids[]": media.json.id,
      }),
    });
    assert(mediaPost.status === 201, `media status expected 201, got ${mediaPost.status}: ${mediaPost.text}`);
    createdIds.push(mediaPost.json?.id);
    assert(mediaPost.json?.media_attachments?.[0]?.url === media.json.url, "media attachment did not attach to form status");

    const read = await request(`/api/v1/statuses/${encodeURIComponent(mediaPost.json.id)}`, { auth: true });
    assert(read.status === 200, `status read expected 200, got ${read.status}`);
    assert(read.json?.media_attachments?.[0]?.description === "dais Mastodon client smoke pixel", "media metadata did not round-trip");

    console.log("Mastodon client smoke: PASS");
    console.log(`app=${app.json.client_id}`);
    console.log(`account=${account.json.id}`);
    console.log(`poll=${poll.json.id}`);
    console.log(`media_post=${mediaPost.json.id}`);
  } finally {
    await cleanup();
  }
}

main().catch((error) => {
  console.error(`Mastodon client smoke: FAIL ${error.stack || error.message || error}`);
  process.exit(1);
});
