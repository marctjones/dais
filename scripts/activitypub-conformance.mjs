#!/usr/bin/env node

import { createHash, createSign, generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";

const config = {
  socialBaseUrl: process.env.DAIS_SOCIAL_BASE_URL || "https://social.dais.social",
  pdsBaseUrl: process.env.DAIS_PDS_BASE_URL || "https://pds.dais.social",
  username: process.env.DAIS_USERNAME || "social",
  acctDomain: process.env.DAIS_ACCT_DOMAIN || "social.dais.social",
  primaryAcctDomain: process.env.DAIS_PRIMARY_ACCT_DOMAIN || "dais.social",
  ownerToken:
    process.env.DAIS_OWNER_TOKEN ||
    (process.env.DAIS_OWNER_TOKEN_FILE ? readTokenFile(process.env.DAIS_OWNER_TOKEN_FILE) : ""),
  knownPublicPost:
    process.env.DAIS_PUBLIC_POST_PATH || "/users/social/posts/20260615220558-6fc8b18f",
  knownPrivatePost:
    process.env.DAIS_PRIVATE_POST_PATH || "/users/social/posts/20260608215639-2ddf52c8",
};

const actorPath = `/users/${config.username}`;
const actorUrl = `${config.socialBaseUrl}${actorPath}`;
const publicCollection = "https://www.w3.org/ns/activitystreams#Public";
const tinyPngBase64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";

const results = [];
const cache = new Map();

function requirement(id, level, title, run) {
  return { id, level, title, run };
}

function pass(test, detail = "") {
  results.push({ ...test, status: "PASS", detail });
}

function fail(test, detail) {
  results.push({ ...test, status: "FAIL", detail });
}

function missing(test, detail) {
  results.push({ ...test, status: "MISSING", detail });
}

function info(test, detail) {
  results.push({ ...test, status: "INFO", detail });
}

async function request(pathOrUrl, options = {}) {
  const url = pathOrUrl.startsWith("http")
    ? pathOrUrl
    : `${config.socialBaseUrl}${pathOrUrl}`;
  const headers = { ...(options.headers || {}) };
  if (options.auth && config.ownerToken) {
    headers.Authorization = `Bearer ${config.ownerToken}`;
  }
  const cacheKey = `${options.method || "GET"} ${url} ${JSON.stringify(headers)} ${
    options.body || ""
  } ${options.redirect || ""}`;
  if (!options.noCache && cache.has(cacheKey)) {
    return cache.get(cacheKey);
  }

  const response = await fetch(url, {
    redirect: options.redirect || "follow",
    method: options.method || "GET",
    headers,
    body: options.body,
  });
  const text = await response.text();
  const contentType = response.headers.get("content-type") || "";
  const location = response.headers.get("location") || "";
  const value = {
    url,
    response,
    status: response.status,
    contentType,
    location,
    text,
    json: parseJson(text),
  };
  if (!options.noCache) {
    cache.set(cacheKey, value);
  }
  return value;
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function readTokenFile(path) {
  try {
    return readFileSync(path, "utf8").trim();
  } catch {
    return "";
  }
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isHttpsUrl(value) {
  return typeof value === "string" && value.startsWith("https://");
}

function hasContentType(actual, expected) {
  return actual.toLowerCase().includes(expected.toLowerCase());
}

function arrayIncludes(array, value) {
  return Array.isArray(array) && array.includes(value);
}

function hasSelfLink(jrd, href) {
  return (
    Array.isArray(jrd.links) &&
    jrd.links.some(
      (link) =>
        link.rel === "self" &&
        link.type === "application/activity+json" &&
        link.href === href,
    )
  );
}

function summarizeJson(value) {
  return JSON.stringify(value).slice(0, 220);
}

function base64Url(value) {
  return Buffer.from(value, "utf8").toString("base64url");
}

function digestHeader(body) {
  return `SHA-256=${createHash("sha256").update(body).digest("base64")}`;
}

function xorSha256Hex(values) {
  const digest = Buffer.alloc(32);
  for (const value of values) {
    const hash = createHash("sha256").update(value).digest();
    for (let index = 0; index < digest.length; index += 1) {
      digest[index] ^= hash[index];
    }
  }
  return digest.toString("hex");
}

function signHttpSignature(privateKeyPem, signingString) {
  return createSign("RSA-SHA256").update(signingString).sign(privateKeyPem, "base64");
}

async function signedInboxFixture() {
  const fixture = generateFixtureActor();
  const body = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: `${fixture.actorUrl}#activities/${Date.now()}`,
    type: "View",
    actor: fixture.actorUrl,
    object: fixture.actorUrl,
  });
  return signedActivityPost(fixture, body);
}

function generateFixtureActor() {
  const { privateKey, publicKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
    privateKeyEncoding: { type: "pkcs8", format: "pem" },
    publicKeyEncoding: { type: "spki", format: "pem" },
  });
  const actorUrl = `${config.socialBaseUrl}/__dais-fixtures/activitypub/actor?pk=${base64Url(publicKey)}`;
  return { actorUrl, privateKey };
}

async function signedActivityPost(fixture, body) {
  const inboxPath = `${actorPath}/inbox`;
  const host = new URL(config.socialBaseUrl).host;
  const date = new Date().toUTCString();
  const digest = digestHeader(body);
  const headersToSign = ["(request-target)", "host", "date", "digest", "content-type"];
  const signingString = [
    `(request-target): post ${inboxPath}`,
    `host: ${host}`,
    `date: ${date}`,
    `digest: ${digest}`,
    "content-type: application/activity+json",
  ].join("\n");
  const signature = signHttpSignature(fixture.privateKey, signingString);
  const signatureHeader =
    `keyId="${fixture.actorUrl}#main-key",algorithm="rsa-sha256",headers="${headersToSign.join(" ")}",signature="${signature}"`;
  return request(inboxPath, {
    method: "POST",
    headers: {
      "Content-Type": "application/activity+json",
      Date: date,
      Digest: digest,
      Signature: signatureHeader,
    },
    body,
  });
}

async function ownerApi(path, options = {}) {
  const res = await request(`/api/dais/owner${path}`, {
    ...options,
    noCache: true,
    headers: {
      ...(options.headers || {}),
      Authorization: `Bearer ${config.ownerToken}`,
      "Content-Type": "application/json",
    },
  });
  if (res.status < 200 || res.status >= 300) {
    throw new Error(`owner API ${path} returned ${res.status}: ${res.text}`);
  }
  return res;
}

async function mastodonApi(path, options = {}) {
  if (!config.ownerToken) {
    return {
      skipped: true,
      detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live Mastodon-backed ActivityPub content fixture",
    };
  }
  const res = await request(path, {
    ...options,
    auth: true,
    headers: {
      ...(options.headers || {}),
      "Content-Type": "application/json",
    },
  });
  return res;
}

async function signedGetFixture(fixture, path, accept = "application/activity+json") {
  const host = new URL(config.socialBaseUrl).host;
  const date = new Date().toUTCString();
  const headersToSign = ["(request-target)", "host", "date", "accept"];
  const signingString = [
    `(request-target): get ${path}`,
    `host: ${host}`,
    `date: ${date}`,
    `accept: ${accept}`,
  ].join("\n");
  const signature = signHttpSignature(fixture.privateKey, signingString);
  return request(path, {
    headers: {
      Accept: accept,
      Date: date,
      Signature:
        `keyId="${fixture.actorUrl}#main-key",algorithm="rsa-sha256",headers="${headersToSign.join(" ")}",signature="${signature}"`,
    },
  });
}

async function authorizedFetchFixture() {
  if (!config.ownerToken) {
    return { skipped: true, detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live authorized-fetch fixture" };
  }
  const fixture = generateFixtureActor();
  const followId = `${fixture.actorUrl}#activities/follow-${Date.now()}`;
  const follow = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: followId,
    type: "Follow",
    actor: fixture.actorUrl,
    object: actorUrl,
  });
  const undo = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: `${fixture.actorUrl}#activities/undo-${Date.now()}`,
    type: "Undo",
    actor: fixture.actorUrl,
    object: {
      id: followId,
      type: "Follow",
      actor: fixture.actorUrl,
      object: actorUrl,
    },
  });
  try {
    const followRes = await signedActivityPost(fixture, follow);
    if (followRes.status < 200 || followRes.status >= 300) {
      throw new Error(`signed Follow expected 2xx, got ${followRes.status}: ${followRes.text}`);
    }
    await ownerApi("/followers/status", {
      method: "POST",
      body: JSON.stringify({ follower_actor_id: fixture.actorUrl, status: "approved" }),
    });
    const signedGet = await signedGetFixture(fixture, config.knownPrivatePost);
    return { signedGet };
  } finally {
    await signedActivityPost(fixture, undo).catch(() => {});
  }
}

async function signedPrivateMediaFixture() {
  if (!config.ownerToken) {
    return { skipped: true, detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live signed private media fixture" };
  }
  const fixture = generateFixtureActor();
  const followId = `${fixture.actorUrl}#activities/follow-${Date.now()}`;
  const follow = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: followId,
    type: "Follow",
    actor: fixture.actorUrl,
    object: actorUrl,
  });
  const undo = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: `${fixture.actorUrl}#activities/undo-${Date.now()}`,
    type: "Undo",
    actor: fixture.actorUrl,
    object: {
      id: followId,
      type: "Follow",
      actor: fixture.actorUrl,
      object: actorUrl,
    },
  });
  let mediaUrl = "";
  let createdId = "";
  try {
    const followRes = await signedActivityPost(fixture, follow);
    if (followRes.status < 200 || followRes.status >= 300) {
      throw new Error(`signed Follow expected 2xx, got ${followRes.status}: ${followRes.text}`);
    }
    await ownerApi("/followers/status", {
      method: "POST",
      body: JSON.stringify({ follower_actor_id: fixture.actorUrl, status: "approved" }),
    });
    const media = await ownerApi("/media", {
      method: "POST",
      body: JSON.stringify({
        filename: "signed-private-media.png",
        media_type: "image/png",
        access: "private",
        require_authorized_fetch: true,
        data_base64: tinyPngBase64,
      }),
    });
    mediaUrl = media.json?.url || "";
    if (!mediaUrl.includes("/media/_private_signed/")) {
      throw new Error(`signed private upload returned unexpected URL: ${mediaUrl}`);
    }
    const created = await ownerApi("/posts", {
      method: "POST",
      body: JSON.stringify({
        text: "signed private media conformance fixture",
        visibility: "followers",
        protocol: "activitypub",
        attachments: [media.json.attachment],
      }),
    });
    createdId = created.json?.id || "";
    const mediaPath = new URL(mediaUrl).pathname;
    const unsigned = await request(mediaPath, { headers: { Accept: "image/png" } });
    const signed = await signedGetFixture(fixture, mediaPath, "image/png");
    return { unsigned, signed, mediaUrl };
  } finally {
    if (createdId) {
      await ownerApi(`/posts/${encodeURIComponent(createdId)}`, {
        method: "DELETE",
      }).catch(() => {});
    }
    if (mediaUrl) {
      await ownerApi("/media/revoke", {
        method: "POST",
        body: JSON.stringify({ url: mediaUrl }),
      }).catch(() => {});
    }
    await signedActivityPost(fixture, undo).catch(() => {});
  }
}

async function followerSynchronizationFixture() {
  if (!config.ownerToken) {
    return { skipped: true, detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live follower synchronization fixture" };
  }
  const fixture = generateFixtureActor();
  const followId = `${fixture.actorUrl}#activities/follow-${Date.now()}`;
  const follow = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: followId,
    type: "Follow",
    actor: fixture.actorUrl,
    object: actorUrl,
  });
  const undo = JSON.stringify({
    "@context": "https://www.w3.org/ns/activitystreams",
    id: `${fixture.actorUrl}#activities/undo-${Date.now()}`,
    type: "Undo",
    actor: fixture.actorUrl,
    object: {
      id: followId,
      type: "Follow",
      actor: fixture.actorUrl,
      object: actorUrl,
    },
  });
  try {
    const followRes = await signedActivityPost(fixture, follow);
    if (followRes.status < 200 || followRes.status >= 300) {
      throw new Error(`signed Follow expected 2xx, got ${followRes.status}: ${followRes.text}`);
    }
    await ownerApi("/followers/status", {
      method: "POST",
      body: JSON.stringify({ follower_actor_id: fixture.actorUrl, status: "approved" }),
    });
    const domain = new URL(fixture.actorUrl).hostname;
    const path = `${actorPath}/followers_synchronization?domain=${domain}`;
    const unsigned = await request(path, { headers: { Accept: "application/activity+json" } });
    const signed = await signedGetFixture(fixture, path);
    return { unsigned, signed, fixture, domain };
  } finally {
    await signedActivityPost(fixture, undo).catch(() => {});
  }
}

async function ownerDiscoveryFixture() {
  if (!config.ownerToken) {
    return { skipped: true, detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner discovery fixture" };
  }
  const fixture = generateFixtureActor();
  const discovery = await ownerApi("/discovery/actor", {
    method: "POST",
    body: JSON.stringify({ target: fixture.actorUrl }),
  });
  return discovery.json;
}

function knownPublicObjectId() {
  return config.knownPublicPost.startsWith("http")
    ? config.knownPublicPost
    : `${config.socialBaseUrl}${config.knownPublicPost}`;
}

async function ownerPostDetail(objectId) {
  const detail = await ownerApi(`/posts/${encodeURIComponent(objectId)}`);
  return detail.json;
}

async function ownerInteraction(objectId, interaction) {
  const response = await ownerApi("/interactions", {
    method: "POST",
    body: JSON.stringify({ object_id: objectId, interaction }),
  });
  return response.json;
}

async function ownerReaderInteractionFixture() {
  if (!config.ownerToken) {
    return { skipped: true, detail: "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner reader interaction fixture" };
  }
  const objectId = knownPublicObjectId();
  await ownerInteraction(objectId, "unlike").catch(() => {});
  await ownerInteraction(objectId, "unboost").catch(() => {});
  const before = await ownerPostDetail(objectId);
  let like;
  let boost;
  try {
    like = await ownerInteraction(objectId, "like");
    boost = await ownerInteraction(objectId, "boost");
    const after = await ownerPostDetail(objectId);
    return { objectId, before, like, boost, after };
  } finally {
    await ownerInteraction(objectId, "unlike").catch(() => {});
    await ownerInteraction(objectId, "unboost").catch(() => {});
  }
}

function rkeyFromAtUri(uri) {
  return typeof uri === "string" ? uri.split("/").pop() : "";
}

async function runRequirement(test) {
  try {
    await test.run({
      pass: (detail) => pass(test, detail),
      fail: (detail) => fail(test, detail),
      missing: (detail) => missing(test, detail),
      info: (detail) => info(test, detail),
    });
  } catch (error) {
    fail(test, error?.stack || String(error));
  }
}

const tests = [
  requirement("WEBFINGER-RFC7033-01", "SPEC", "WebFinger returns JRD for acct URI", async (t) => {
    const res = await request(
      `/.well-known/webfinger?resource=acct:${config.username}@${config.acctDomain}`,
      { headers: { Accept: "application/jrd+json" } },
    );
    if (res.status !== 200) return t.fail(`expected HTTP 200, got ${res.status}`);
    if (!hasContentType(res.contentType, "application/jrd+json")) {
      return t.fail(`expected application/jrd+json, got ${res.contentType || "none"}`);
    }
    if (!isObject(res.json)) return t.fail("response is not JSON");
    if (!hasSelfLink(res.json, actorUrl)) {
      return t.fail(`missing self application/activity+json link to ${actorUrl}`);
    }
    t.pass(res.json.subject || "JRD present");
  }),

  requirement("AP-ACTOR-01", "SPEC", "Actor document is an ActivityStreams actor", async (t) => {
    const res = await request(actorPath, {
      headers: { Accept: "application/activity+json, application/ld+json" },
    });
    if (res.status !== 200) return t.fail(`expected HTTP 200, got ${res.status}`);
    if (!hasContentType(res.contentType, "application/activity+json")) {
      return t.fail(`expected ActivityPub content type, got ${res.contentType || "none"}`);
    }
    const actor = res.json;
    if (!isObject(actor)) return t.fail("actor response is not JSON");
    const required = ["@context", "type", "id", "preferredUsername", "inbox", "outbox"];
    const missingFields = required.filter((field) => actor[field] === undefined);
    if (missingFields.length) return t.fail(`missing fields: ${missingFields.join(", ")}`);
    if (actor.type !== "Person") return t.fail(`expected Person, got ${actor.type}`);
    if (actor.id !== actorUrl) return t.fail(`expected id ${actorUrl}, got ${actor.id}`);
    if (!isHttpsUrl(actor.inbox) || !isHttpsUrl(actor.outbox)) {
      return t.fail("inbox/outbox must be HTTPS URLs");
    }
    t.pass(actor.id);
  }),

  requirement("MASTODON-ACTOR-01", "MASTODON", "Actor exposes Mastodon-compatible public key", async (t) => {
    const res = await request(actorPath, {
      headers: { Accept: "application/activity+json" },
    });
    const key = res.json?.publicKey;
    if (!isObject(key)) return t.fail("missing publicKey object");
    if (key.owner !== actorUrl) return t.fail(`publicKey.owner mismatch: ${key.owner}`);
    if (!String(key.id || "").startsWith(`${actorUrl}#`)) {
      return t.fail(`publicKey.id should be actor fragment URL, got ${key.id}`);
    }
    if (!String(key.publicKeyPem || "").includes("BEGIN PUBLIC KEY")) {
      return t.fail("publicKeyPem is missing PEM public key");
    }
    t.pass(key.id);
  }),

  requirement("MASTODON-ACTOR-02", "MASTODON", "Actor marks locked/private-follow posture", async (t) => {
    const res = await request(actorPath, {
      headers: { Accept: "application/activity+json" },
    });
    if (res.json?.manuallyApprovesFollowers !== true) {
      return t.fail("expected manuallyApprovesFollowers=true for private-by-default account");
    }
    t.pass("manuallyApprovesFollowers=true");
  }),

  requirement("HTTP-NEGOTIATION-01", "SPEC", "Actor negotiates browser HTML and explicit JSON", async (t) => {
    const html = await request(actorPath, { headers: { Accept: "text/html" } });
    if (html.status !== 200 || !hasContentType(html.contentType, "text/html")) {
      return t.fail(`browser request expected HTML 200, got ${html.status} ${html.contentType}`);
    }
    const json = await request(`${actorPath}?format=json`, { headers: { Accept: "text/html" } });
    if (json.status !== 200 || !hasContentType(json.contentType, "application/activity+json")) {
      return t.fail(`format=json expected ActivityPub JSON 200, got ${json.status} ${json.contentType}`);
    }
    t.pass("HTML and JSON variants available");
  }),

  requirement("AP-OUTBOX-01", "SPEC", "Outbox is an OrderedCollection of Create activities", async (t) => {
    const res = await request(`${actorPath}/outbox`, {
      headers: { Accept: "application/activity+json" },
    });
    if (res.status !== 200) return t.fail(`expected HTTP 200, got ${res.status}`);
    const outbox = res.json;
    if (outbox?.type !== "OrderedCollection") return t.fail(`expected OrderedCollection, got ${outbox?.type}`);
    if (!Number.isInteger(outbox.totalItems)) return t.fail("totalItems must be an integer");
    if (!Array.isArray(outbox.orderedItems)) return t.fail("orderedItems must be an array");
    const bad = outbox.orderedItems.find((item) => item.type !== "Create" || !isObject(item.object));
    if (bad) return t.fail(`bad outbox item: ${summarizeJson(bad)}`);
    t.pass(`${outbox.totalItems} items`);
  }),

  requirement("DAIS-PRIVACY-01", "DAIS-PRIVACY", "Anonymous outbox excludes encrypted fallback posts", async (t) => {
    const res = await request(`${actorPath}/outbox`, {
      headers: { Accept: "application/activity+json" },
    });
    const leaked = (res.json?.orderedItems || []).filter((item) => {
      const content = item?.object?.content || "";
      return content.includes("End-to-end encrypted message") || item?.object?.encryptedMessage;
    });
    if (leaked.length) return t.fail(`encrypted/fallback items leaked: ${summarizeJson(leaked[0])}`);
    t.pass("no encrypted/fallback items in public outbox");
  }),

  requirement("AP-OBJECT-01", "SPEC", "Public object dereferences as Note JSON", async (t) => {
    const res = await request(config.knownPublicPost, {
      headers: { Accept: "application/activity+json" },
    });
    if (res.status !== 200) return t.fail(`expected HTTP 200, got ${res.status}`);
    const note = res.json;
    if (note?.type !== "Note") return t.fail(`expected Note, got ${note?.type}`);
    if (note.attributedTo !== actorUrl) return t.fail(`attributedTo mismatch: ${note.attributedTo}`);
    if (!arrayIncludes(note.to, publicCollection)) return t.fail("public Note must address AS Public");
    t.pass(note.id);
  }),

  requirement("DAIS-PRIVACY-02", "DAIS-PRIVACY", "Known private/E2EE object is not anonymously dereferenceable", async (t) => {
    const html = await request(config.knownPrivatePost, { headers: { Accept: "text/html" } });
    const json = await request(config.knownPrivatePost, {
      headers: { Accept: "application/activity+json" },
    });
    if (html.status !== 404 || json.status !== 404) {
      return t.fail(`expected anonymous 404 for both HTML/JSON, got ${html.status}/${json.status}`);
    }
    t.pass("anonymous private/E2EE dereference denied");
  }),

  requirement("AP-COLLECTIONS-01", "SPEC", "Followers/following collections have ActivityStreams shape", async (t) => {
    const followers = await request(`${actorPath}/followers`, {
      headers: { Accept: "application/activity+json" },
    });
    const following = await request(`${actorPath}/following`, {
      headers: { Accept: "application/activity+json" },
    });
    for (const [name, res] of [
      ["followers", followers],
      ["following", following],
    ]) {
      if (res.status !== 200) return t.fail(`${name}: expected HTTP 200, got ${res.status}`);
      if (res.json?.type !== "OrderedCollection") {
        return t.fail(`${name}: expected OrderedCollection, got ${res.json?.type}`);
      }
      if (!Number.isInteger(res.json.totalItems)) return t.fail(`${name}: totalItems must be integer`);
      if (!isHttpsUrl(res.json.first)) return t.fail(`${name}: first page must be HTTPS URL`);
    }
    t.pass("followers/following summaries valid");
  }),

  requirement("DAIS-PRIVACY-03", "DAIS-PRIVACY", "Anonymous social graph pages do not expose actor IDs", async (t) => {
    const followers = await request(`${actorPath}/followers?page=1`, {
      headers: { Accept: "application/activity+json" },
    });
    const following = await request(`${actorPath}/following?page=1`, {
      headers: { Accept: "application/activity+json" },
    });
    const followerItems = followers.json?.orderedItems;
    const followingItems = following.json?.orderedItems;
    if (!Array.isArray(followerItems) || followerItems.length !== 0) {
      return t.fail(`followers page leaked items: ${summarizeJson(followerItems)}`);
    }
    if (!Array.isArray(followingItems) || followingItems.length !== 0) {
      return t.fail(`following page leaked items: ${summarizeJson(followingItems)}`);
    }
    t.pass("orderedItems empty for anonymous reads");
  }),

  requirement("AP-INBOX-01", "SPEC", "Inbox allows CORS preflight and rejects unsigned POST", async (t) => {
    const options = await request(`${actorPath}/inbox`, { method: "OPTIONS" });
    if (options.status !== 200) return t.fail(`OPTIONS expected 200, got ${options.status}`);
    const post = await request(`${actorPath}/inbox`, {
      method: "POST",
      headers: { "Content-Type": "application/activity+json" },
      body: "{}",
    });
    if (post.status !== 401) return t.fail(`unsigned POST expected 401, got ${post.status}`);
    t.pass("preflight ok; unsigned POST rejected");
  }),

  requirement("MASTODON-SECURITY-01", "MASTODON", "Signed inbox delivery verification is implemented", async (t) => {
    const res = await signedInboxFixture();
    if (res.status < 200 || res.status >= 300) {
      return t.fail(`signed fixture POST expected 2xx, got ${res.status}: ${res.text}`);
    }
    t.pass("valid signed POST with Digest accepted by deployed inbox");
  }),

  requirement("MASTODON-SECURITY-02", "MASTODON", "Authorized fetch for private posts is implemented", async (t) => {
    const result = await authorizedFetchFixture();
    if (result.skipped) return t.info(result.detail);
    const res = result.signedGet;
    if (res.status !== 200) {
      return t.fail(`signed approved-follower GET expected 200, got ${res.status}: ${res.text}`);
    }
    if (!isObject(res.json) || res.json.id !== `${config.socialBaseUrl}${config.knownPrivatePost}`) {
      return t.fail(`private post response mismatch: ${summarizeJson(res.json)}`);
    }
    t.pass("valid signed approved-follower GET can fetch private post");
  }),

  requirement("MASTODON-SECURITY-03", "MASTODON", "Private media supports recipient-bound authorized fetch", async (t) => {
    const result = await signedPrivateMediaFixture();
    if (result.skipped) return t.info(result.detail);
    if (result.unsigned.status !== 401) {
      return t.fail(`unsigned signed-media GET expected 401, got ${result.unsigned.status}: ${result.unsigned.text}`);
    }
    if (result.signed.status !== 200) {
      return t.fail(`signed approved-follower media GET expected 200, got ${result.signed.status}: ${result.signed.text}`);
    }
    if (!hasContentType(result.signed.contentType, "image/png")) {
      return t.fail(`signed media expected image/png, got ${result.signed.contentType || "none"}`);
    }
    t.pass("signed approved follower can fetch private media while unsigned fetch is denied");
  }),

  requirement("MASTODON-SYNC-01", "MASTODON", "Signed partial follower synchronization collection is available", async (t) => {
    const result = await followerSynchronizationFixture();
    if (result.skipped) return t.info(result.detail);
    if (result.unsigned.status !== 401) {
      return t.fail(`unsigned follower synchronization GET expected 401, got ${result.unsigned.status}: ${result.unsigned.text}`);
    }
    const res = result.signed;
    if (res.status !== 200) {
      return t.fail(`signed follower synchronization GET expected 200, got ${res.status}: ${res.text}`);
    }
    if (!hasContentType(res.contentType, "application/activity+json")) {
      return t.fail(`expected ActivityPub JSON, got ${res.contentType || "none"}`);
    }
    const collection = res.json;
    if (collection?.type !== "OrderedCollection") {
      return t.fail(`expected OrderedCollection, got ${collection?.type}`);
    }
    if (!Array.isArray(collection.orderedItems)) {
      return t.fail("orderedItems must be an array");
    }
    if (!collection.orderedItems.includes(result.fixture.actorUrl)) {
      return t.fail(`partial collection did not include approved fixture follower: ${summarizeJson(collection)}`);
    }
    const crossDomain = collection.orderedItems.find((actor) => new URL(actor).hostname !== result.domain);
    if (crossDomain) {
      return t.fail(`partial collection leaked another domain actor: ${crossDomain}`);
    }
    const digest = xorSha256Hex([...collection.orderedItems].sort());
    if (!/^[0-9a-f]{64}$/.test(digest)) {
      return t.fail(`digest calculation failed: ${digest}`);
    }
    t.pass(`signed partial collection for ${result.domain} has ${collection.orderedItems.length} follower(s); digest=${digest}`);
  }),

  requirement("MASTODON-CONTENT-01", "MASTODON", "Mastodon status payload basics are present", async (t) => {
    const res = await request(config.knownPublicPost, {
      headers: { Accept: "application/activity+json" },
    });
    const note = res.json;
    const required = ["id", "type", "attributedTo", "content", "published", "to"];
    const missingFields = required.filter((field) => note?.[field] === undefined);
    if (missingFields.length) return t.fail(`missing fields: ${missingFields.join(", ")}`);
    if (note.type !== "Note") return t.fail(`expected Note, got ${note.type}`);
    t.pass("Note has Mastodon-consumed fields");
  }),

  requirement("MASTODON-CONTENT-02", "MASTODON", "Mastodon optional status collections are exposed", async (t) => {
    const res = await request(config.knownPublicPost, {
      headers: { Accept: "application/activity+json" },
    });
    const missingFields = ["replies", "likes", "shares"].filter((field) => res.json?.[field] === undefined);
    if (missingFields.length) {
      return t.missing(`optional Mastodon collections not exposed: ${missingFields.join(", ")}`);
    }
    t.pass("replies/likes/shares present");
  }),

  requirement("MASTODON-CONTENT-03", "MASTODON", "Live public Question exposes media, tags, summary, and poll shape", async (t) => {
    if (!config.ownerToken) {
      return t.info("set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live rich content fixture");
    }
    let createdId = "";
    try {
      const media = await mastodonApi("/api/v1/media", {
        method: "POST",
        body: JSON.stringify({
          filename: "dais-activitypub-rich-content.png",
          media_type: "image/png",
          data_base64: tinyPngBase64,
          description: "ActivityPub rich content fixture image",
        }),
      });
      if (media.skipped) return t.info(media.detail);
      if (media.status !== 200) {
        return t.fail(`media upload expected 200, got ${media.status}: ${media.text}`);
      }

      const token = `DaisApRich${Date.now()}`;
      const create = await mastodonApi("/api/v1/statuses", {
        method: "POST",
        body: JSON.stringify({
          status: `dais ActivityPub rich content fixture @social@dais.social #${token}`,
          spoiler_text: "ActivityPub rich content fixture summary",
          visibility: "public",
          media_ids: [media.json.id],
          poll: {
            options: ["Alpha", "Beta"],
            multiple: false,
            expires_in: 300,
          },
        }),
      });
      if (create.status !== 201) {
        return t.fail(`status create expected 201, got ${create.status}: ${create.text}`);
      }
      createdId = create.json?.id || "";
      const postPath = new URL(createdId).pathname;
      const object = await request(postPath, {
        headers: { Accept: "application/activity+json" },
      });
      if (object.status !== 200) {
        return t.fail(`ActivityPub object fetch expected 200, got ${object.status}: ${object.text}`);
      }
      const note = object.json;
      if (note?.type !== "Question") return t.fail(`expected Question, got ${note?.type}`);
      if (note.summary !== "ActivityPub rich content fixture summary") {
        return t.fail(`summary did not round-trip: ${summarizeJson(note)}`);
      }
      if (!note.contentMap?.en?.includes("dais ActivityPub rich content fixture")) {
        return t.fail("contentMap.en missing rendered content");
      }
      if (!Array.isArray(note.oneOf) || note.oneOf.length !== 2 || note.oneOf[0]?.name !== "Alpha") {
        return t.fail(`poll oneOf shape incomplete: ${summarizeJson(note.oneOf)}`);
      }
      if (!Array.isArray(note.attachment) || note.attachment[0]?.mediaType !== "image/png") {
        return t.fail(`image attachment shape incomplete: ${summarizeJson(note.attachment)}`);
      }
      if (!note.tag?.some((tag) => tag.type === "Mention" && tag.name === "@social@dais.social")) {
        return t.fail(`mention tag missing: ${summarizeJson(note.tag)}`);
      }
      if (!note.tag?.some((tag) => tag.type === "Hashtag" && tag.name === `#${token}`)) {
        return t.fail(`hashtag tag missing: ${summarizeJson(note.tag)}`);
      }
      t.pass(`temporary public Question ${createdId} exposed rich Mastodon ActivityPub shape`);
    } finally {
      if (createdId) {
        await mastodonApi(`/api/v1/statuses/${encodeURIComponent(createdId)}`, {
          method: "DELETE",
        }).catch(() => {});
      }
    }
  }),

  requirement("OWNER-DISCOVERY-01", "DAIS-OWNER", "Actor discovery returns recent public post previews when available", async (t) => {
    const actor = await ownerDiscoveryFixture();
    if (actor?.skipped) return t.info(actor.detail);
    if (!actor?.id || !actor?.inbox) {
      return t.fail(`discovery response missing actor shape: ${summarizeJson(actor)}`);
    }
    const posts = actor.recent_public_posts || [];
    if (!Array.isArray(posts) || posts.length === 0) {
      return t.fail(`discovery response missing recent_public_posts: ${summarizeJson(actor)}`);
    }
    const preview = posts[0];
    if (preview.type !== "Note" || !preview.content.includes("Dais fixture public preview post")) {
      return t.fail(`unexpected public preview post: ${summarizeJson(preview)}`);
    }
    t.pass("owner discovery returned fixture actor profile and recent public post preview");
  }),

  requirement("OWNER-READER-01", "DAIS-OWNER", "Reader like and boost actions enqueue ActivityPub deliveries and update detail counts", async (t) => {
    const result = await ownerReaderInteractionFixture();
    if (result.skipped) return t.info(result.detail);
    if (result.like?.interaction !== "like" || !Array.isArray(result.like.delivery_ids) || result.like.delivery_ids.length === 0) {
      return t.fail(`like did not return delivery ids: ${summarizeJson(result.like)}`);
    }
    if (result.boost?.interaction !== "boost" || !Array.isArray(result.boost.delivery_ids) || result.boost.delivery_ids.length === 0) {
      return t.fail(`boost did not return delivery ids: ${summarizeJson(result.boost)}`);
    }
    const beforeLikes = Number(result.before?.like_count || 0);
    const beforeBoosts = Number(result.before?.boost_count || 0);
    const afterLikes = Number(result.after?.like_count || 0);
    const afterBoosts = Number(result.after?.boost_count || 0);
    if (afterLikes !== beforeLikes + 1) {
      return t.fail(`like_count expected ${beforeLikes + 1}, got ${afterLikes}`);
    }
    if (afterBoosts !== beforeBoosts + 1) {
      return t.fail(`boost_count expected ${beforeBoosts + 1}, got ${afterBoosts}`);
    }
    t.pass(`like=${result.like.delivery_ids.length} boost=${result.boost.delivery_ids.length} delivery row(s) for ${result.objectId}`);
  }),

  requirement("PDS-ATPROTO-01", "MASTODON-ADJACENT", "ATProto public read endpoints stay available", async (t) => {
    const describe = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.server.describeServer`, {
      headers: { Accept: "application/json" },
    });
    const did = describe.json?.did || `did:web:${config.acctDomain}`;
    const repo = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.getRepo?did=did:web:${config.acctDomain}`, {
      headers: { Accept: "application/json" },
    });
    const feed = await request(`${config.pdsBaseUrl}/xrpc/app.bsky.feed.getAuthorFeed?actor=${config.acctDomain}`, {
      headers: { Accept: "application/json" },
    });
    const status = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.getRepoStatus?did=${encodeURIComponent(did)}`, {
      headers: { Accept: "application/json" },
    });
    const repos = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.listRepos`, {
      headers: { Accept: "application/json" },
    });
    const describedRepo = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.repo.describeRepo?repo=${encodeURIComponent(did)}`, {
      headers: { Accept: "application/json" },
    });
    const subscribeStatus = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.subscribeRepos`, {
      headers: { Accept: "application/json" },
    });
    const statuses = [describe, repo, feed, status, repos, describedRepo, subscribeStatus].map((res) => res.status);
    if (statuses.some((statusCode) => statusCode !== 200)) {
      return t.fail(`expected 200s, got ${statuses.join("/")}`);
    }
    if (status.json?.did !== did || !Array.isArray(repos.json?.repos) || !Array.isArray(describedRepo.json?.collections)) {
      return t.fail("PDS repo metadata shape is incomplete");
    }
    const firstPost = feed.json?.feed?.[0]?.post;
    const rkey = rkeyFromAtUri(firstPost?.uri);
    if (rkey) {
      const record = await request(
        `${config.pdsBaseUrl}/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(rkey)}`,
        { headers: { Accept: "application/json" } },
      );
      if (record.status !== 200 || record.json?.value?.$type !== "app.bsky.feed.post") {
        return t.fail(`getRecord expected feed post, got ${record.status} ${summarizeJson(record.json)}`);
      }
    }
    t.pass("PDS identity, repo, record, feed, and subscribe status endpoints return compatible JSON");
  }),
];

for (const test of tests) {
  await runRequirement(test);
}

const counts = results.reduce(
  (acc, result) => {
    acc[result.status] = (acc[result.status] || 0) + 1;
    return acc;
  },
  { PASS: 0, FAIL: 0, MISSING: 0, INFO: 0 },
);

const byLevel = results.reduce((acc, result) => {
  acc[result.level] ||= { PASS: 0, FAIL: 0, MISSING: 0, INFO: 0 };
  acc[result.level][result.status] = (acc[result.level][result.status] || 0) + 1;
  return acc;
}, {});

console.log("\nActivityPub/Mastodon conformance report");
console.log(`Target: ${config.socialBaseUrl} actor=${actorUrl}`);
console.log("Sources: W3C ActivityPub, ActivityStreams 2.0, RFC 7033 WebFinger, Mastodon federation docs");
console.log("");

for (const result of results) {
  const marker = {
    PASS: "PASS",
    FAIL: "FAIL",
    MISSING: "MISSING",
    INFO: "INFO",
  }[result.status];
  console.log(`${marker.padEnd(7)} ${result.level.padEnd(16)} ${result.id.padEnd(24)} ${result.title}`);
  if (result.detail) console.log(`        ${result.detail}`);
}

console.log("");
console.log("Summary:");
console.log(`  PASS=${counts.PASS || 0} FAIL=${counts.FAIL || 0} MISSING=${counts.MISSING || 0} INFO=${counts.INFO || 0}`);
for (const [level, levelCounts] of Object.entries(byLevel)) {
  console.log(
    `  ${level}: PASS=${levelCounts.PASS || 0} FAIL=${levelCounts.FAIL || 0} MISSING=${
      levelCounts.MISSING || 0
    } INFO=${levelCounts.INFO || 0}`,
  );
}

if (counts.FAIL > 0) {
  process.exit(1);
}
