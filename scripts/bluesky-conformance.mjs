#!/usr/bin/env node

const config = {
  pdsBaseUrl: process.env.DAIS_PDS_BASE_URL || "https://pds.dais.social",
  acctDomain: process.env.DAIS_ACCT_DOMAIN || "social.dais.social",
};

const results = [];

function requirement(id, title, run) {
  return { id, title, run };
}

function record(test, status, detail = "") {
  results.push({ ...test, status, detail });
}

async function request(path, options = {}) {
  const target = path.startsWith("http") ? path : `${config.pdsBaseUrl}${path}`;
  const response = await fetch(target, {
    method: options.method || "GET",
    headers: options.headers || { Accept: "application/json" },
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

function expectArray(value, label) {
  if (!Array.isArray(value)) throw new Error(`${label} is not an array`);
}

function expectObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} is not an object`);
  }
}

function did() {
  return `did:web:${config.acctDomain}`;
}

function rkeyFromAtUri(uri) {
  const value = String(uri || "");
  const parts = value.split("/");
  return parts.length ? parts.at(-1) : "";
}

function assertPublicPostShape(post) {
  expectObject(post, "post");
  if (!String(post.uri || "").startsWith(`at://${did()}/app.bsky.feed.post/`)) {
    throw new Error(`unexpected post URI ${post.uri}`);
  }
  if (!post.cid) throw new Error("post missing cid");
  if (post.author?.did !== did()) throw new Error("post author DID mismatch");
  if (post.author?.handle !== config.acctDomain) throw new Error("post author handle mismatch");
  if (post.record?.$type !== "app.bsky.feed.post") throw new Error("post record type mismatch");
  if (!post.record?.createdAt) throw new Error("post record missing createdAt");
  if (String(post.record?.text || "").includes("End-to-end encrypted message")) {
    throw new Error(`encrypted fallback text leaked through Bluesky feed: ${post.uri}`);
  }
}

const tests = [
  requirement("BLUESKY-ID-01", "PDS identity and DID document expose compatible shapes", async () => {
    const server = await request("/xrpc/com.atproto.server.describeServer");
    const didDoc = await request("/.well-known/did.json");
    if (server.status !== 200 || didDoc.status !== 200) {
      throw new Error(`expected 200/200, got ${server.status}/${didDoc.status}`);
    }
    if (server.json?.did !== did()) throw new Error(`describeServer DID mismatch: ${server.json?.did}`);
    if (!server.json?.availableUserDomains?.includes(config.acctDomain)) {
      throw new Error("availableUserDomains missing account domain");
    }
    if (didDoc.json?.id !== did()) throw new Error("DID document id mismatch");
    const service = didDoc.json?.service?.find((entry) => entry.id === "#atproto_pds");
    if (service?.serviceEndpoint !== config.pdsBaseUrl) throw new Error("DID PDS service endpoint mismatch");
  }),

  requirement("BLUESKY-REPO-01", "Repo status, listRepos, describeRepo, and getRepo expose the public repo floor", async () => {
    const status = await request(`/xrpc/com.atproto.sync.getRepoStatus?did=${encodeURIComponent(did())}`);
    const repos = await request("/xrpc/com.atproto.sync.listRepos");
    const repo = await request(`/xrpc/com.atproto.repo.describeRepo?repo=${encodeURIComponent(did())}`);
    const car = await request(`/xrpc/com.atproto.sync.getRepo?did=${encodeURIComponent(did())}`);
    const statuses = [status, repos, repo, car].map((res) => res.status);
    if (statuses.some((value) => value !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    if (status.json?.did !== did() || status.json?.status !== "active") throw new Error("repo status shape mismatch");
    if (!repos.json?.repos?.some((entry) => entry.did === did())) throw new Error("listRepos missing dais DID");
    if (!repo.json?.collections?.includes("app.bsky.feed.post")) throw new Error("describeRepo missing feed.post collection");
    if (car.json?.did !== did() || !car.json?.warning || !Number.isInteger(car.json?.records)) {
      throw new Error("getRepo compatibility floor shape mismatch");
    }
  }),

  requirement("BLUESKY-FEED-01", "Author feed, timeline, and getRecord expose lexicon-shaped public posts", async () => {
    const authorFeed = await request(`/xrpc/app.bsky.feed.getAuthorFeed?actor=${encodeURIComponent(did())}&limit=2`);
    const timeline = await request("/xrpc/app.bsky.feed.getTimeline?limit=2");
    if (authorFeed.status !== 200 || timeline.status !== 200) {
      throw new Error(`expected 200/200, got ${authorFeed.status}/${timeline.status}`);
    }
    expectArray(authorFeed.json?.feed, "author feed");
    expectArray(timeline.json?.feed, "timeline feed");
    const first = authorFeed.json.feed[0]?.post || timeline.json.feed[0]?.post;
    if (!first) throw new Error("no public Bluesky feed posts available for getRecord check");
    assertPublicPostShape(first);
    const rkey = rkeyFromAtUri(first.uri);
    const record = await request(
      `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(rkey)}`,
    );
    if (record.status !== 200) throw new Error(`getRecord expected 200, got ${record.status}`);
    if (record.json?.uri !== first.uri || record.json?.value?.$type !== "app.bsky.feed.post") {
      throw new Error("getRecord did not match feed post");
    }
  }),

  requirement("BLUESKY-PROFILE-01", "Profile endpoints expose local account shape and counts", async () => {
    const profile = await request(`/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did())}`);
    const profiles = await request(
      `/xrpc/app.bsky.actor.getProfiles?actors=${encodeURIComponent(did())}&actors=${encodeURIComponent(config.acctDomain)}`,
    );
    if (profile.status !== 200 || profiles.status !== 200) {
      throw new Error(`expected 200/200, got ${profile.status}/${profiles.status}`);
    }
    if (profile.json?.did !== did() || profile.json?.handle !== config.acctDomain) {
      throw new Error("profile identity shape mismatch");
    }
    for (const field of ["followersCount", "followsCount", "postsCount"]) {
      if (!Number.isInteger(profile.json?.[field])) throw new Error(`profile missing integer ${field}`);
    }
    expectArray(profiles.json?.profiles, "profiles");
    if (profiles.json.profiles.length !== 2) throw new Error("getProfiles did not return both requested profiles");
    if (!profiles.json.profiles.every((item) => item.did === did())) {
      throw new Error("getProfiles local identities did not resolve to dais DID");
    }
  }),

  requirement("BLUESKY-APPVIEW-01", "Personal AppView read endpoints return client-safe arrays", async () => {
    const notifications = await request("/xrpc/app.bsky.notification.listNotifications?limit=5");
    const followers = await request(`/xrpc/app.bsky.graph.getFollowers?actor=${encodeURIComponent(did())}&limit=5`);
    const follows = await request(`/xrpc/app.bsky.graph.getFollows?actor=${encodeURIComponent(did())}&limit=5`);
    const likes = await request(`/xrpc/app.bsky.feed.getLikes?uri=${encodeURIComponent(`at://${did()}/app.bsky.feed.post/placeholder`)}&limit=5`);
    const statuses = [notifications, followers, follows, likes].map((res) => res.status);
    if (statuses.some((value) => value !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    expectArray(notifications.json?.notifications, "notifications");
    expectArray(followers.json?.followers, "followers");
    expectArray(follows.json?.follows, "follows");
    expectArray(likes.json?.likes, "likes");
  }),

  requirement("BLUESKY-PRIVACY-01", "PDS public feeds exclude private/E2EE fallback content", async () => {
    const feed = await request("/xrpc/app.bsky.feed.getTimeline?limit=20");
    if (feed.status !== 200) throw new Error(`timeline expected 200, got ${feed.status}`);
    expectArray(feed.json?.feed, "timeline feed");
    for (const item of feed.json.feed) {
      assertPublicPostShape(item.post);
    }
  }),

  requirement("BLUESKY-SYNC-01", "subscribeRepos non-WebSocket request returns explicit WebSocket guidance", async () => {
    const res = await request("/xrpc/com.atproto.sync.subscribeRepos");
    if (res.status !== 200) throw new Error(`expected 200, got ${res.status}`);
    if (res.json?.transport !== "websocket" || res.json?.status !== "available") {
      throw new Error("subscribeRepos guidance shape mismatch");
    }
  }),

  requirement("BLUESKY-ERROR-01", "Unsupported repo collections fail explicitly", async () => {
    const res = await request(
      `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.like&rkey=missing`,
    );
    if (res.status !== 404) throw new Error(`expected 404 for unsupported collection, got ${res.status}`);
  }),
];

for (const test of tests) {
  try {
    await test.run();
    record(test, "PASS");
  } catch (error) {
    record(test, "FAIL", error.message || String(error));
  }
}

let pass = 0;
let fail = 0;
console.log("\nBluesky compatibility report");
console.log(`Target: ${config.pdsBaseUrl}`);
for (const result of results) {
  if (result.status === "PASS") pass += 1;
  if (result.status === "FAIL") fail += 1;
  console.log(`${result.status.padEnd(5)} ${result.id.padEnd(18)} ${result.title}${result.detail ? ` - ${result.detail}` : ""}`);
}
console.log(`\nSummary: PASS=${pass} FAIL=${fail}`);
if (fail > 0) process.exit(1);
