#!/usr/bin/env node

const config = {
  baseUrl: process.env.DAIS_SOCIAL_BASE_URL || "https://social.dais.social",
  token: process.env.DAIS_MASTODON_BEARER_TOKEN || "",
};

const tinyPngBase64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
const tinyMp4Base64 = "AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAAIZnJlZQ==";

const results = [];

function requirement(id, auth, title, run) {
  return { id, auth, title, run };
}

function record(test, status, detail = "") {
  results.push({ ...test, status, detail });
}

async function request(path, options = {}) {
  const headers = new Headers(options.headers || {});
  if (options.auth && config.token) headers.set("Authorization", `Bearer ${config.token}`);
  const response = await fetch(`${config.baseUrl}${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body,
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
    contentType: response.headers.get("content-type") || "",
    text,
    json,
  };
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function expectArray(value, label) {
  if (!Array.isArray(value)) throw new Error(`${label} is not an array`);
}

const tests = [
  requirement("MASTODON-API-INSTANCE-01", false, "Instance v1/v2 endpoints expose compatible JSON", async () => {
    const v1 = await request("/api/v1/instance");
    const v2 = await request("/api/v2/instance");
    if (v1.status !== 200 || v2.status !== 200) throw new Error(`expected 200/200, got ${v1.status}/${v2.status}`);
    if (!v1.json?.uri || !v2.json?.configuration?.statuses) throw new Error("instance shape incomplete");
  }),
  requirement("MASTODON-API-APPS-01", false, "App registration and OAuth token compatibility flow works", async () => {
    const app = await request("/api/v1/apps", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ client_name: "dais conformance", redirect_uris: "urn:ietf:wg:oauth:2.0:oob" }),
    });
    const token = await request("/oauth/token", { method: "POST" });
    if (app.status !== 200 || token.status !== 200) throw new Error(`expected 200/200, got ${app.status}/${token.status}`);
    if (!app.json?.client_id || token.json?.access_token !== "owner-token-required") {
      throw new Error("OAuth compatibility shape incomplete or leaked a non-placeholder token");
    }
  }),
  requirement("MASTODON-API-DISCOVERY-01", false, "OAuth and NodeInfo discovery metadata expose client-safe shapes", async () => {
    const oauth = await request("/.well-known/oauth-authorization-server");
    const openid = await request("/.well-known/openid-configuration");
    const nodeInfoDiscovery = await request("/.well-known/nodeinfo");
    const nodeInfo = await request("/nodeinfo/2.0");
    const statuses = [oauth, openid, nodeInfoDiscovery, nodeInfo].map((res) => res.status);
    if (statuses.some((status) => status !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    if (!oauth.json?.authorization_endpoint || !oauth.json?.token_endpoint || oauth.json.issuer !== config.baseUrl) {
      throw new Error("OAuth metadata shape incomplete");
    }
    if (openid.json?.issuer !== oauth.json.issuer || openid.json?.token_endpoint !== oauth.json.token_endpoint) {
      throw new Error("OpenID metadata did not match OAuth metadata");
    }
    if (!Array.isArray(nodeInfoDiscovery.json?.links) || !nodeInfoDiscovery.json.links[0]?.href) {
      throw new Error("NodeInfo discovery shape incomplete");
    }
    if (nodeInfo.json?.software?.name !== "dais" || !nodeInfo.json?.usage?.users) {
      throw new Error("NodeInfo document shape incomplete");
    }
  }),
  requirement("MASTODON-API-PUBLIC-01", false, "Public timelines and statuses privacy-filter public content", async () => {
    const timeline = await request("/api/v1/timelines/public?limit=2");
    if (timeline.status !== 200) throw new Error(`timeline expected 200, got ${timeline.status}`);
    expectArray(timeline.json, "public timeline");
    for (const status of timeline.json) {
      if (status.visibility !== "public") throw new Error(`non-public status leaked: ${status.id}`);
      if (String(status.content || "").includes("End-to-end encrypted message")) {
        throw new Error(`encrypted fallback leaked: ${status.id}`);
      }
    }
  }),
  requirement("MASTODON-API-COMPAT-01", false, "Unauthenticated compatibility endpoints fail closed where required", async () => {
    const verify = await request("/api/v1/accounts/verify_credentials");
    const home = await request("/api/v1/timelines/home");
    if (verify.status !== 401 || home.status !== 401) throw new Error(`expected 401/401, got ${verify.status}/${home.status}`);
  }),
  requirement("MASTODON-API-AUTH-01", true, "Authenticated account, timeline, preferences, and notifications work", async () => {
    const account = await request("/api/v1/accounts/verify_credentials", { auth: true });
    const home = await request("/api/v1/timelines/home?limit=2", { auth: true });
    const preferences = await request("/api/v1/preferences", { auth: true });
    const notifications = await request("/api/v1/notifications?limit=2", { auth: true });
    const statuses = [account, home, preferences, notifications].map((res) => res.status);
    if (statuses.some((status) => status !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    if (!account.json?.id || !isObject(preferences.json)) throw new Error("authenticated shapes incomplete");
    expectArray(home.json, "home timeline");
    expectArray(notifications.json, "notifications");
  }),
  requirement("MASTODON-API-READ-01", true, "Search, relationships, filters, lists, and conversations have client-safe shapes", async () => {
    const account = await request("/api/v1/accounts/verify_credentials", { auth: true });
    const id = encodeURIComponent(account.json.id);
    const checks = await Promise.all([
      request(`/api/v1/accounts/relationships?id[]=${id}`, { auth: true }),
      request("/api/v2/search?q=dais&type=statuses", { auth: true }),
      request("/api/v2/filters", { auth: true }),
      request("/api/v1/lists", { auth: true }),
      request("/api/v1/conversations", { auth: true }),
      request("/api/v1/bookmarks", { auth: true }),
      request("/api/v1/markers", { auth: true }),
    ]);
    if (checks.some((res) => res.status !== 200)) throw new Error(`expected all 200, got ${checks.map((res) => res.status).join("/")}`);
    expectArray(checks[0].json, "relationships");
    if (!isObject(checks[1].json) || !Array.isArray(checks[1].json.statuses)) throw new Error("search shape incomplete");
    expectArray(checks[2].json, "filters");
    expectArray(checks[3].json, "lists");
    expectArray(checks[4].json, "conversations");
    expectArray(checks[5].json, "bookmarks");
    if (!isObject(checks[6].json)) throw new Error("markers shape incomplete");
  }),
  requirement("MASTODON-API-READ-02", true, "Account graph, status context, favourites, moderation, and streaming shapes work", async () => {
    const account = await request("/api/v1/accounts/verify_credentials", { auth: true });
    const accountId = encodeURIComponent(account.json.id);
    const timeline = await request("/api/v1/timelines/public?limit=1");
    const statusId = timeline.json?.[0]?.id ? encodeURIComponent(timeline.json[0].id) : "";
    const checks = await Promise.all([
      request(`/api/v1/accounts/${accountId}`, { auth: true }),
      request(`/api/v1/accounts/${accountId}/statuses?limit=1`, { auth: true }),
      request(`/api/v1/accounts/${accountId}/followers?limit=2`, { auth: true }),
      request(`/api/v1/accounts/${accountId}/following?limit=2`, { auth: true }),
      request("/api/v1/favourites?limit=2", { auth: true }),
      request("/api/v1/blocks?limit=2", { auth: true }),
      request("/api/v1/mutes?limit=2", { auth: true }),
      request("/api/v1/streaming/user", { auth: true }),
      request("/api/v1/reports", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ account_id: account.json.id, comment: "dais conformance compatibility report" }),
      }),
      statusId ? request(`/api/v1/statuses/${statusId}/context`, { auth: true }) : Promise.resolve({ status: 200, json: { ancestors: [], descendants: [] } }),
    ]);
    if (checks.some((res) => res.status !== 200 && res.status !== 201)) {
      throw new Error(`expected 200/201, got ${checks.map((res) => res.status).join("/")}`);
    }
    if (!checks[0].json?.id) throw new Error("account lookup shape incomplete");
    expectArray(checks[1].json, "account statuses");
    expectArray(checks[2].json, "followers");
    expectArray(checks[3].json, "following");
    expectArray(checks[4].json, "favourites");
    expectArray(checks[5].json, "blocks");
    expectArray(checks[6].json, "mutes");
    if (!checks[7].contentType.includes("text/event-stream")) throw new Error("streaming content-type is not event-stream");
    if (!checks[8].json?.id || checks[8].json.action_taken !== false) throw new Error("report shape incomplete");
    if (!Array.isArray(checks[9].json?.ancestors) || !Array.isArray(checks[9].json?.descendants)) {
      throw new Error("status context shape incomplete");
    }
  }),
  requirement("MASTODON-API-WRITE-01", true, "Status creation accepts Mastodon poll parameters and returns poll shape", async () => {
    let createdId = "";
    try {
      const create = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon poll conformance ${new Date().toISOString()}`,
          visibility: "public",
          poll: {
            options: ["Yes", "No"],
            multiple: false,
            expires_in: 300,
          },
        }),
      });
      if (create.status !== 201) throw new Error(`create expected 201, got ${create.status}: ${create.text}`);
      createdId = create.json?.id || "";
      if (!create.json?.poll || create.json.poll.multiple !== false) throw new Error("created status missing poll shape");
      if (!Array.isArray(create.json.poll.options) || create.json.poll.options.length !== 2) {
        throw new Error("created poll options shape incomplete");
      }

      const read = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { auth: true });
      if (read.status !== 200) throw new Error(`read expected 200, got ${read.status}`);
      if (!read.json?.poll || read.json.poll.options?.[0]?.title !== "Yes") {
        throw new Error("stored poll did not round-trip through status read");
      }
    } finally {
      if (createdId) {
        await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { method: "DELETE", auth: true });
      }
    }
  }),
  requirement("MASTODON-API-WRITE-02", true, "Media upload can be attached to a public status and round-trips as media_attachments", async () => {
    let createdId = "";
    try {
      const media = await request("/api/v1/media", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          filename: "dais-conformance.png",
          media_type: "image/png",
          data_base64: tinyPngBase64,
          description: "dais conformance pixel",
        }),
      });
      if (media.status !== 200) throw new Error(`media upload expected 200, got ${media.status}: ${media.text}`);
      if (!media.json?.id || media.json.type !== "image" || !media.json.url) throw new Error("media upload shape incomplete");

      const create = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon media conformance ${new Date().toISOString()}`,
          visibility: "public",
          media_ids: [media.json.id],
        }),
      });
      if (create.status !== 201) throw new Error(`media status create expected 201, got ${create.status}: ${create.text}`);
      createdId = create.json?.id || "";
      if (!Array.isArray(create.json?.media_attachments) || create.json.media_attachments.length !== 1) {
        throw new Error("created status missing media attachment");
      }
      if (create.json.media_attachments[0].type !== "image") throw new Error("created status media attachment is not image");

      const read = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { auth: true });
      if (read.status !== 200) throw new Error(`media status read expected 200, got ${read.status}`);
      if (!Array.isArray(read.json?.media_attachments) || read.json.media_attachments[0]?.url !== media.json.url) {
        throw new Error("media attachment did not round-trip through status read");
      }
    } finally {
      if (createdId) {
        await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { method: "DELETE", auth: true });
      }
    }
  }),
  requirement("MASTODON-API-WRITE-05", true, "Video media upload is advertised and round-trips as a video attachment", async () => {
    let createdId = "";
    try {
      const instance = await request("/api/v2/instance");
      const supported = instance.json?.configuration?.media_attachments?.supported_mime_types || [];
      if (!supported.includes("video/mp4") || !supported.includes("video/webm")) {
        throw new Error("instance does not advertise supported video MIME types");
      }

      const media = await request("/api/v1/media", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          filename: "dais-conformance.mp4",
          media_type: "video/mp4",
          data_base64: tinyMp4Base64,
          description: "dais conformance video",
        }),
      });
      if (media.status !== 200) throw new Error(`video upload expected 200, got ${media.status}: ${media.text}`);
      if (!media.json?.id || media.json.type !== "video" || !media.json.url) throw new Error("video upload shape incomplete");

      const create = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon video conformance ${new Date().toISOString()}`,
          visibility: "public",
          media_ids: [media.json.id],
        }),
      });
      if (create.status !== 201) throw new Error(`video status create expected 201, got ${create.status}: ${create.text}`);
      createdId = create.json?.id || "";
      if (create.json?.media_attachments?.[0]?.type !== "video") {
        throw new Error("created status media attachment is not video");
      }

      const read = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { auth: true });
      if (read.status !== 200) throw new Error(`video status read expected 200, got ${read.status}`);
      if (read.json?.media_attachments?.[0]?.type !== "video" || read.json.media_attachments[0].url !== media.json.url) {
        throw new Error("video attachment did not round-trip through status read");
      }
    } finally {
      if (createdId) {
        await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { method: "DELETE", auth: true });
      }
    }
  }),
  requirement("MASTODON-API-WRITE-03", true, "Reply creation round-trips through status read and context descendants", async () => {
    let parentId = "";
    let replyId = "";
    try {
      const parent = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon reply parent conformance ${new Date().toISOString()}`,
          visibility: "public",
        }),
      });
      if (parent.status !== 201) throw new Error(`parent create expected 201, got ${parent.status}: ${parent.text}`);
      parentId = parent.json?.id || "";

      const reply = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon reply child conformance ${new Date().toISOString()}`,
          visibility: "public",
          in_reply_to_id: parentId,
        }),
      });
      if (reply.status !== 201) throw new Error(`reply create expected 201, got ${reply.status}: ${reply.text}`);
      replyId = reply.json?.id || "";
      if (reply.json?.in_reply_to_id !== parentId) throw new Error("created reply missing in_reply_to_id");

      const read = await request(`/api/v1/statuses/${encodeURIComponent(replyId)}`, { auth: true });
      if (read.status !== 200) throw new Error(`reply read expected 200, got ${read.status}`);
      if (read.json?.in_reply_to_id !== parentId) throw new Error("reply did not round-trip in_reply_to_id");

      const context = await request(`/api/v1/statuses/${encodeURIComponent(parentId)}/context`, { auth: true });
      if (context.status !== 200) throw new Error(`context expected 200, got ${context.status}`);
      if (!Array.isArray(context.json?.descendants) || !context.json.descendants.some((status) => status.id === replyId)) {
        throw new Error("parent context descendants did not include reply");
      }
    } finally {
      if (replyId) {
        await request(`/api/v1/statuses/${encodeURIComponent(replyId)}`, { method: "DELETE", auth: true });
      }
      if (parentId) {
        await request(`/api/v1/statuses/${encodeURIComponent(parentId)}`, { method: "DELETE", auth: true });
      }
    }
  }),
  requirement("MASTODON-API-WRITE-04", true, "Favourite and reblog actions update returned status state", async () => {
    let createdId = "";
    try {
      const create = await request("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: `dais Mastodon interaction conformance ${new Date().toISOString()}`,
          visibility: "public",
        }),
      });
      if (create.status !== 201) throw new Error(`create expected 201, got ${create.status}: ${create.text}`);
      createdId = create.json?.id || "";

      const favourite = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}/favourite`, { method: "POST", auth: true });
      if (favourite.status !== 200) throw new Error(`favourite expected 200, got ${favourite.status}: ${favourite.text}`);
      if (favourite.json?.favourited !== true || Number(favourite.json?.favourites_count || 0) < 1) {
        throw new Error("favourite response did not reflect favourited state");
      }

      const reblog = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}/reblog`, { method: "POST", auth: true });
      if (reblog.status !== 200) throw new Error(`reblog expected 200, got ${reblog.status}: ${reblog.text}`);
      if (reblog.json?.reblogged !== true || Number(reblog.json?.reblogs_count || 0) < 1) {
        throw new Error("reblog response did not reflect reblogged state");
      }

      const read = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { auth: true });
      if (read.status !== 200) throw new Error(`read expected 200, got ${read.status}`);
      if (read.json?.favourited !== true || read.json?.reblogged !== true) {
        throw new Error("status read did not retain interaction state");
      }

      const unfavourite = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}/unfavourite`, { method: "POST", auth: true });
      const unreblog = await request(`/api/v1/statuses/${encodeURIComponent(createdId)}/unreblog`, { method: "POST", auth: true });
      if (unfavourite.status !== 200 || unreblog.status !== 200) {
        throw new Error(`undo interactions expected 200/200, got ${unfavourite.status}/${unreblog.status}`);
      }
      if (unfavourite.json?.favourited !== false || unreblog.json?.reblogged !== false) {
        throw new Error("undo interaction responses did not clear state");
      }
    } finally {
      if (createdId) {
        await request(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { method: "DELETE", auth: true });
      }
    }
  }),
];

for (const test of tests) {
  if (test.auth && !config.token) {
    record(test, "SKIP", "set DAIS_MASTODON_BEARER_TOKEN for authenticated checks");
    continue;
  }
  try {
    await test.run();
    record(test, "PASS");
  } catch (error) {
    record(test, "FAIL", error?.stack || String(error));
  }
}

console.log("\nMastodon API compatibility report");
console.log(`Target: ${config.baseUrl}`);
for (const result of results) {
  console.log(`${result.status.padEnd(5)} ${result.id.padEnd(24)} ${result.title}`);
  if (result.detail) console.log(`      ${result.detail}`);
}

const failed = results.filter((result) => result.status === "FAIL");
const skipped = results.filter((result) => result.status === "SKIP");
console.log(`\nSummary: PASS=${results.filter((result) => result.status === "PASS").length} FAIL=${failed.length} SKIP=${skipped.length}`);
if (failed.length > 0) process.exit(1);
