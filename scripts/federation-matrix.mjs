#!/usr/bin/env node

const config = {
  socialBaseUrl: process.env.DAIS_SOCIAL_BASE_URL || "https://social.dais.social",
  pdsBaseUrl: process.env.DAIS_PDS_BASE_URL || "https://pds.dais.social",
  username: process.env.DAIS_USERNAME || "social",
  acctDomain: process.env.DAIS_ACCT_DOMAIN || "social.dais.social",
  knownPublicPost:
    process.env.DAIS_PUBLIC_POST_PATH || "/users/social/posts/20260608212713-5dafca61",
  knownPrivatePost:
    process.env.DAIS_PRIVATE_POST_PATH || "/users/social/posts/20260608215639-2ddf52c8",
  mastodonApiToken: process.env.DAIS_MASTODON_API_TOKEN || "",
  remoteTargets: parseRemoteTargets(process.env.DAIS_FEDERATION_TARGETS || ""),
};

const actorPath = `/users/${config.username}`;
const actorUrl = `${config.socialBaseUrl}${actorPath}`;
const publicCollection = "https://www.w3.org/ns/activitystreams#Public";
const results = [];
const cache = new Map();

function parseRemoteTargets(raw) {
  if (!raw.trim()) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      throw new Error("DAIS_FEDERATION_TARGETS must be a JSON array");
    }
    return parsed;
  } catch (error) {
    console.error(`Invalid DAIS_FEDERATION_TARGETS: ${error.message}`);
    process.exit(2);
  }
}

function row(area, target, capability, status, detail = "") {
  results.push({ area, target, capability, status, detail });
}

async function request(pathOrUrl, options = {}) {
  const url = pathOrUrl.startsWith("http")
    ? pathOrUrl
    : `${config.socialBaseUrl}${pathOrUrl}`;
  const cacheKey = `${options.method || "GET"} ${url} ${JSON.stringify(options.headers || {})} ${
    options.body || ""
  } ${options.redirect || ""}`;
  if (cache.has(cacheKey)) return cache.get(cacheKey);

  const response = await fetch(url, {
    redirect: options.redirect || "follow",
    method: options.method || "GET",
    headers: options.headers || {},
    body: options.body,
  });
  const text = await response.text();
  const value = {
    url,
    status: response.status,
    ok: response.ok,
    contentType: response.headers.get("content-type") || "",
    location: response.headers.get("location") || "",
    text,
    json: parseJson(text),
  };
  cache.set(cacheKey, value);
  return value;
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function hasContentType(actual, expected) {
  return actual.toLowerCase().includes(expected.toLowerCase());
}

function actorSelfLink(jrd) {
  return asArray(jrd?.links).find(
    (link) => link.rel === "self" && link.type === "application/activity+json",
  );
}

function summarize(value) {
  if (value === undefined || value === null) return "";
  return String(value).replaceAll("\n", " ").slice(0, 180);
}

function rkeyFromAtUri(uri) {
  return typeof uri === "string" ? uri.split("/").pop() : "";
}

async function record(area, target, capability, fn) {
  try {
    const detail = await fn();
    row(area, target, capability, "PASS", detail);
  } catch (error) {
    row(area, target, capability, "FAIL", error?.message || String(error));
  }
}

function info(area, target, capability, detail) {
  row(area, target, capability, "INFO", detail);
}

async function daisBaseline() {
  await record("dais", config.acctDomain, "WebFinger acct discovery", async () => {
    const res = await request(
      `/.well-known/webfinger?resource=acct:${config.username}@${config.acctDomain}`,
      { headers: { Accept: "application/jrd+json" } },
    );
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (!hasContentType(res.contentType, "application/jrd+json")) {
      throw new Error(`expected application/jrd+json, got ${res.contentType || "none"}`);
    }
    const link = actorSelfLink(res.json);
    if (!link || link.href !== actorUrl) throw new Error(`missing self link to ${actorUrl}`);
    return res.json.subject;
  });

  await record("dais", actorUrl, "ActivityPub actor and signing key", async () => {
    const res = await request(actorPath, {
      headers: { Accept: "application/activity+json, application/ld+json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    const actor = res.json;
    if (!isObject(actor)) throw new Error("actor is not JSON");
    for (const field of ["@context", "id", "type", "inbox", "outbox", "publicKey"]) {
      if (actor[field] === undefined) throw new Error(`missing ${field}`);
    }
    if (actor.id !== actorUrl) throw new Error(`id mismatch: ${actor.id}`);
    if (actor.type !== "Person") throw new Error(`expected Person, got ${actor.type}`);
    if (!String(actor.publicKey?.publicKeyPem || "").includes("BEGIN PUBLIC KEY")) {
      throw new Error("missing PEM public key");
    }
    return actor.publicKey.id;
  });

  await record("dais", `${actorUrl}/outbox`, "Anonymous outbox excludes private/E2EE content", async () => {
    const res = await request(`${actorPath}/outbox`, {
      headers: { Accept: "application/activity+json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (res.json?.type !== "OrderedCollection") {
      throw new Error(`expected OrderedCollection, got ${res.json?.type}`);
    }
    const leaked = asArray(res.json.orderedItems).find((item) => {
      const object = item?.object || {};
      const content = object.content || "";
      return object.encryptedMessage || content.includes("End-to-end encrypted message");
    });
    if (leaked) throw new Error(`leaked private/E2EE item ${summarize(leaked.id || leaked.object?.id)}`);
    return `${res.json.totalItems} public items`;
  });

  await record("dais", config.knownPublicPost, "Public Note dereference", async () => {
    const res = await request(config.knownPublicPost, {
      headers: { Accept: "application/activity+json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    const note = res.json;
    if (note?.type !== "Note") throw new Error(`expected Note, got ${note?.type}`);
    if (!asArray(note.to).includes(publicCollection)) throw new Error("public Note missing AS Public");
    return note.id;
  });

  await record("dais", config.knownPrivatePost, "Anonymous private/E2EE denial", async () => {
    const html = await request(config.knownPrivatePost, { headers: { Accept: "text/html" } });
    const json = await request(config.knownPrivatePost, {
      headers: { Accept: "application/activity+json" },
    });
    if (html.status !== 404 || json.status !== 404) {
      throw new Error(`expected 404/404, got ${html.status}/${json.status}`);
    }
    return "private object not anonymously dereferenceable";
  });

  await record("dais", `${actorUrl}/inbox`, "Unsigned inbox rejection", async () => {
    const res = await request(`${actorPath}/inbox`, {
      method: "POST",
      headers: { "Content-Type": "application/activity+json" },
      body: "{}",
    });
    if (![400, 401, 403].includes(res.status)) {
      throw new Error(`expected 400/401/403, got ${res.status}`);
    }
    return `rejected with HTTP ${res.status}`;
  });
}

async function mastodonApiFloor() {
  await record("mastodon-api", config.socialBaseUrl, "Instance metadata", async () => {
    const res = await request("/api/v1/instance", {
      headers: { Accept: "application/json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (!res.json?.uri) throw new Error("missing uri");
    return `${res.json.uri} ${res.json.version || ""}`.trim();
  });

  await record("mastodon-api", config.socialBaseUrl, "Public timeline is public-only", async () => {
    const res = await request("/api/v1/timelines/public?limit=5", {
      headers: { Accept: "application/json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (!Array.isArray(res.json)) throw new Error("timeline is not an array");
    const leaked = res.json.find(
      (status) =>
        status.visibility !== "public" ||
        status.content?.includes("End-to-end encrypted message") ||
        status.encryptedMessage,
    );
    if (leaked) throw new Error(`unsafe timeline status ${summarize(leaked.id)}`);
    return `${res.json.length} public statuses`;
  });

  await record("mastodon-api", config.socialBaseUrl, "Authenticated home timeline gate", async () => {
    const anon = await request("/api/v1/timelines/home", {
      headers: { Accept: "application/json" },
    });
    if (anon.status !== 401) throw new Error(`anonymous request expected 401, got ${anon.status}`);
    if (!config.mastodonApiToken) return "anonymous denied; token not configured";
    const authed = await request("/api/v1/timelines/home", {
      headers: {
        Accept: "application/json",
        Authorization: `Bearer ${config.mastodonApiToken}`,
      },
    });
    if (authed.status !== 200) throw new Error(`authenticated request expected 200, got ${authed.status}`);
    return `authenticated rows ${Array.isArray(authed.json) ? authed.json.length : "unknown"}`;
  });
}

async function atprotoFloor() {
  await record("atproto", config.pdsBaseUrl, "PDS describeServer", async () => {
    const res = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.server.describeServer`, {
      headers: { Accept: "application/json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (!Array.isArray(res.json?.availableUserDomains)) {
      throw new Error("missing availableUserDomains");
    }
    return res.json.availableUserDomains.join(", ");
  });

  await record("atproto", config.pdsBaseUrl, "PDS repo metadata", async () => {
    const did = `did:web:${config.acctDomain}`;
    const status = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.getRepoStatus?did=${encodeURIComponent(did)}`, {
      headers: { Accept: "application/json" },
    });
    const repos = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.listRepos`, {
      headers: { Accept: "application/json" },
    });
    const repo = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.repo.describeRepo?repo=${encodeURIComponent(did)}`, {
      headers: { Accept: "application/json" },
    });
    if (status.status !== 200 || repos.status !== 200 || repo.status !== 200) {
      throw new Error(`expected 200s, got ${status.status}/${repos.status}/${repo.status}`);
    }
    if (status.json?.did !== did || !asArray(repos.json?.repos).some((entry) => entry.did === did)) {
      throw new Error("repo status/listRepos did not include dais DID");
    }
    if (!asArray(repo.json?.collections).includes("app.bsky.feed.post")) {
      throw new Error("describeRepo missing app.bsky.feed.post");
    }
    return `${status.json.status}; collections ${repo.json.collections.join(", ")}`;
  });

  await record("atproto", config.pdsBaseUrl, "PDS public feed and getRecord", async () => {
    const did = `did:web:${config.acctDomain}`;
    const feed = await request(`${config.pdsBaseUrl}/xrpc/app.bsky.feed.getAuthorFeed?actor=${encodeURIComponent(did)}&limit=1`, {
      headers: { Accept: "application/json" },
    });
    if (feed.status !== 200) throw new Error(`feed expected 200, got ${feed.status}`);
    const firstPost = feed.json?.feed?.[0]?.post;
    const rkey = rkeyFromAtUri(firstPost?.uri);
    if (!rkey) return "feed is reachable; no public posts returned";
    const record = await request(
      `${config.pdsBaseUrl}/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(rkey)}`,
      { headers: { Accept: "application/json" } },
    );
    if (record.status !== 200) throw new Error(`getRecord expected 200, got ${record.status}`);
    if (record.json?.value?.$type !== "app.bsky.feed.post") {
      throw new Error(`unexpected record ${summarize(JSON.stringify(record.json))}`);
    }
    return rkey;
  });

  await record("atproto", config.pdsBaseUrl, "PDS subscribeRepos status", async () => {
    const res = await request(`${config.pdsBaseUrl}/xrpc/com.atproto.sync.subscribeRepos`, {
      headers: { Accept: "application/json" },
    });
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (res.json?.transport !== "websocket") throw new Error("missing websocket transport status");
    return res.json.status;
  });
}

async function remoteTargets() {
  if (config.remoteTargets.length === 0) {
    info(
      "remote",
      "Mastodon/Pleroma/Misskey/Pixelfed",
      "Configured compatibility probes",
      "set DAIS_FEDERATION_TARGETS to a JSON array of {name, acct, actor}",
    );
    return;
  }

  for (const target of config.remoteTargets) {
    const name = target.name || target.acct || target.actor || "remote";
    if (target.acct) {
      await record("remote", name, "Remote WebFinger resolves ActivityPub actor", async () => {
        const domain = target.acct.split("@").at(-1);
        const res = await request(`https://${domain}/.well-known/webfinger?resource=acct:${target.acct}`, {
          headers: { Accept: "application/jrd+json" },
        });
        if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
        const link = actorSelfLink(res.json);
        if (!link?.href) throw new Error("missing ActivityPub self link");
        return link.href;
      });
    } else {
      info("remote", name, "Remote WebFinger resolves ActivityPub actor", "target has no acct");
    }

    if (target.actor) {
      await record("remote", name, "Remote actor has inbox/outbox/publicKey shape", async () => {
        const res = await request(target.actor, {
          headers: { Accept: "application/activity+json, application/ld+json" },
        });
        if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
        const actor = res.json;
        if (!isObject(actor)) throw new Error("actor is not JSON");
        for (const field of ["id", "type", "inbox"]) {
          if (!actor[field]) throw new Error(`missing ${field}`);
        }
        return `${actor.type} ${actor.id}`;
      });
    } else {
      info("remote", name, "Remote actor has inbox/outbox/publicKey shape", "target has no actor URL");
    }
  }
}

function printMarkdown() {
  console.log("| Area | Target | Capability | Status | Detail |");
  console.log("| --- | --- | --- | --- | --- |");
  for (const item of results) {
    console.log(
      `| ${escapeCell(item.area)} | ${escapeCell(item.target)} | ${escapeCell(item.capability)} | ${item.status} | ${escapeCell(item.detail)} |`,
    );
  }
}

function escapeCell(value) {
  return String(value ?? "").replaceAll("|", "\\|").replaceAll("\n", " ");
}

async function main() {
  await daisBaseline();
  await mastodonApiFloor();
  await atprotoFloor();
  await remoteTargets();

  if (process.argv.includes("--json")) {
    console.log(JSON.stringify(results, null, 2));
  } else {
    printMarkdown();
  }

  const failed = results.filter((item) => item.status === "FAIL");
  const infoRows = results.filter((item) => item.status === "INFO");
  console.error(
    `\nFederation matrix: PASS=${results.length - failed.length - infoRows.length} FAIL=${failed.length} INFO=${infoRows.length}`,
  );
  process.exit(failed.length > 0 ? 1 : 0);
}

main();
