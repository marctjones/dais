#!/usr/bin/env node

import WebSocket from "ws";

const config = {
  pdsBaseUrl: process.env.DAIS_PDS_BASE_URL || "https://pds.dais.social",
  socialBaseUrl: process.env.DAIS_SOCIAL_BASE_URL || "https://social.dais.social",
  acctDomain: process.env.DAIS_ACCT_DOMAIN || "social.dais.social",
  mastodonToken: process.env.DAIS_MASTODON_BEARER_TOKEN || "",
};

const tinyPngBase64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";

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

async function socialRequest(path, options = {}) {
  const headers = new Headers(options.headers || {});
  if (options.auth && config.mastodonToken) headers.set("Authorization", `Bearer ${config.mastodonToken}`);
  const response = await fetch(`${config.socialBaseUrl}${path}`, {
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

function websocketUrl(path) {
  const base = new URL(config.pdsBaseUrl);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = path;
  base.search = "";
  return base.toString();
}

function subscribeReposMessages(timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const messages = [];
    const ws = new WebSocket(websocketUrl("/xrpc/com.atproto.sync.subscribeRepos"));
    const timeout = setTimeout(() => {
      ws.close();
      reject(new Error(`subscribeRepos WebSocket timed out after ${timeoutMs}ms with ${messages.length} message(s)`));
    }, timeoutMs);
    ws.on("message", (data) => {
      const text = data.toString();
      try {
        messages.push(JSON.parse(text));
      } catch {
        messages.push({ parseError: true, text });
      }
      if (messages.some((message) => message.t === "#info") && messages.some((message) => message.t === "#commit")) {
        clearTimeout(timeout);
        ws.close();
        resolve(messages);
      }
    });
    ws.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    ws.on("close", () => {
      if (messages.some((message) => message.t === "#info") && messages.some((message) => message.t === "#commit")) {
        clearTimeout(timeout);
        resolve(messages);
      }
    });
  });
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
    const latest = await request(`/xrpc/com.atproto.sync.getLatestCommit?did=${encodeURIComponent(did())}`);
    const repos = await request("/xrpc/com.atproto.sync.listRepos");
    const repo = await request(`/xrpc/com.atproto.repo.describeRepo?repo=${encodeURIComponent(did())}`);
    const car = await request(`/xrpc/com.atproto.sync.getRepo?did=${encodeURIComponent(did())}`);
    const blobs = await request(`/xrpc/com.atproto.sync.listBlobs?did=${encodeURIComponent(did())}&limit=5`);
    const statuses = [status, latest, repos, repo, car, blobs].map((res) => res.status);
    if (statuses.some((value) => value !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    if (status.json?.did !== did() || status.json?.status !== "active") throw new Error("repo status shape mismatch");
    if (!latest.json?.cid || !latest.json?.rev) throw new Error("getLatestCommit shape mismatch");
    if (latest.json.cid !== status.json?.head || latest.json.rev !== status.json?.rev) {
      throw new Error("getLatestCommit and getRepoStatus metadata diverged");
    }
    if (!repos.json?.repos?.some((entry) => entry.did === did())) throw new Error("listRepos missing dais DID");
    for (const collection of [
      "app.bsky.actor.profile",
      "app.bsky.feed.post",
      "app.bsky.feed.like",
      "app.bsky.feed.repost",
      "app.bsky.graph.follow",
    ]) {
      if (!repo.json?.collections?.includes(collection)) throw new Error(`describeRepo missing ${collection} collection`);
    }
    if (car.json?.did !== did() || !car.json?.warning || !Number.isInteger(car.json?.records)) {
      throw new Error("getRepo compatibility floor shape mismatch");
    }
    expectArray(blobs.json?.cids, "listBlobs cids");
  }),

  requirement("BLUESKY-REPO-02", "Repo metadata advances when exposed record collections change", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for repo metadata fixture" };
    }

    const beforeStatus = await request(`/xrpc/com.atproto.sync.getRepoStatus?did=${encodeURIComponent(did())}`);
    const beforeRepo = await request(`/xrpc/com.atproto.sync.getRepo?did=${encodeURIComponent(did())}`);
    if (beforeStatus.status !== 200 || beforeRepo.status !== 200) {
      throw new Error(`repo metadata before expected 200/200, got ${beforeStatus.status}/${beforeRepo.status}`);
    }

    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const subject = `did:web:repo-stats-${Date.now()}.example.com`;
    let rkey = "";
    try {
      const create = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.graph.follow",
          record: {
            $type: "app.bsky.graph.follow",
            subject,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (create.status !== 200) throw new Error(`follow createRecord expected 200, got ${create.status}: ${create.text}`);
      rkey = rkeyFromAtUri(create.json?.uri);

      const afterStatus = await request(`/xrpc/com.atproto.sync.getRepoStatus?did=${encodeURIComponent(did())}`);
      const afterRepo = await request(`/xrpc/com.atproto.sync.getRepo?did=${encodeURIComponent(did())}`);
      if (afterStatus.status !== 200 || afterRepo.status !== 200) {
        throw new Error(`repo metadata after expected 200/200, got ${afterStatus.status}/${afterRepo.status}`);
      }
      if (afterRepo.json.records !== beforeRepo.json.records + 1) {
        throw new Error(`repo record count did not include follow record: before=${beforeRepo.json.records} after=${afterRepo.json.records}`);
      }
      if (afterStatus.json.rev === beforeStatus.json.rev || afterStatus.json.head === beforeStatus.json.head) {
        throw new Error("repo status rev/head did not advance after record create");
      }
      if (afterRepo.json.rev !== afterStatus.json.rev || afterRepo.json.head !== afterStatus.json.head) {
        throw new Error("getRepo and getRepoStatus metadata diverged after record create");
      }
    } finally {
      if (rkey) {
        const remove = await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: "app.bsky.graph.follow",
            rkey,
          }),
        });
        if (remove.status !== 200) {
          throw new Error(`follow deleteRecord expected 200, got ${remove.status}: ${remove.text}`);
        }
      }
    }

    const restoredRepo = await request(`/xrpc/com.atproto.sync.getRepo?did=${encodeURIComponent(did())}`);
    if (restoredRepo.status !== 200) throw new Error(`repo metadata restored expected 200, got ${restoredRepo.status}`);
    if (restoredRepo.json.records !== beforeRepo.json.records) {
      throw new Error(`repo record count did not return after cleanup: before=${beforeRepo.json.records} restored=${restoredRepo.json.records}`);
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

  requirement("BLUESKY-CURSOR-01", "Public feed endpoints support cursor pagination", async () => {
    const firstTimeline = await request("/xrpc/app.bsky.feed.getTimeline?limit=1");
    if (firstTimeline.status !== 200) throw new Error(`timeline page 1 expected 200, got ${firstTimeline.status}`);
    expectArray(firstTimeline.json?.feed, "timeline page 1");
    if (firstTimeline.json.feed.length !== 1) throw new Error("timeline page 1 did not honor limit=1");
    if (!firstTimeline.json?.cursor) throw new Error("timeline page 1 did not return a next cursor");

    const secondTimeline = await request(
      `/xrpc/app.bsky.feed.getTimeline?limit=1&cursor=${encodeURIComponent(firstTimeline.json.cursor)}`,
    );
    if (secondTimeline.status !== 200) throw new Error(`timeline page 2 expected 200, got ${secondTimeline.status}`);
    expectArray(secondTimeline.json?.feed, "timeline page 2");
    if (secondTimeline.json.feed.length !== 1) throw new Error("timeline page 2 did not honor limit=1");
    if (secondTimeline.json.feed[0]?.post?.uri === firstTimeline.json.feed[0]?.post?.uri) {
      throw new Error("timeline cursor returned a duplicate first-page post");
    }

    const author = await request(`/xrpc/app.bsky.feed.getAuthorFeed?actor=${encodeURIComponent(did())}&limit=1`);
    if (author.status !== 200) throw new Error(`author feed page 1 expected 200, got ${author.status}`);
    if (!author.json?.cursor) throw new Error("author feed did not return a next cursor");
    const authorNext = await request(
      `/xrpc/app.bsky.feed.getAuthorFeed?actor=${encodeURIComponent(did())}&limit=1&cursor=${encodeURIComponent(author.json.cursor)}`,
    );
    if (authorNext.status !== 200) throw new Error(`author feed page 2 expected 200, got ${authorNext.status}`);
    if (authorNext.json?.feed?.[0]?.post?.uri === author.json?.feed?.[0]?.post?.uri) {
      throw new Error("author feed cursor returned a duplicate first-page post");
    }
  }),

  requirement("BLUESKY-REPO-CURSOR-01", "Repo listRecords supports cursor pagination", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for repo listRecords cursor check" };
    }
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const records = await request(
      `/xrpc/com.atproto.repo.listRecords?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&limit=1`,
      { headers: { Authorization: `Bearer ${session.json.accessJwt}` } },
    );
    if (records.status !== 200) throw new Error(`listRecords page 1 expected 200, got ${records.status}: ${records.text}`);
    expectArray(records.json?.records, "listRecords page 1");
    if (!records.json?.cursor) throw new Error("listRecords page 1 did not return a next cursor");
    const nextRecords = await request(
      `/xrpc/com.atproto.repo.listRecords?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&limit=1&cursor=${encodeURIComponent(records.json.cursor)}`,
      { headers: { Authorization: `Bearer ${session.json.accessJwt}` } },
    );
    if (nextRecords.status !== 200) {
      throw new Error(`listRecords page 2 expected 200, got ${nextRecords.status}: ${nextRecords.text}`);
    }
    if (nextRecords.json?.records?.[0]?.uri === records.json?.records?.[0]?.uri) {
      throw new Error("listRecords cursor returned a duplicate first-page record");
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

  requirement("BLUESKY-PROFILE-RECORD-01", "Owner-token actor.profile record round-trips through repo and AppView reads", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for profile record fixture" };
    }

    const before = await request(`/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did())}`);
    if (before.status !== 200) throw new Error(`profile before expected 200, got ${before.status}: ${before.text}`);
    const previousDisplayName = before.json?.displayName || "";
    const previousDescription = before.json?.description || "";

    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const displayName = `dais profile conformance ${Date.now()}`;
    const description = "temporary Dais PDS profile conformance description";
    const writeProfile = async (nextDisplayName, nextDescription) =>
      request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.actor.profile",
          rkey: "self",
          record: {
            $type: "app.bsky.actor.profile",
            displayName: nextDisplayName,
            description: nextDescription,
          },
        }),
      });

    try {
      const create = await writeProfile(displayName, description);
      if (create.status !== 200) throw new Error(`profile createRecord expected 200, got ${create.status}: ${create.text}`);
      if (create.json?.uri !== `at://${did()}/app.bsky.actor.profile/self`) throw new Error("profile record URI mismatch");

      const record = await request(
        `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.actor.profile&rkey=self`,
      );
      if (record.status !== 200) throw new Error(`profile getRecord expected 200, got ${record.status}: ${record.text}`);
      if (record.json?.value?.displayName !== displayName || record.json?.value?.description !== description) {
        throw new Error("profile record did not round-trip displayName/description");
      }

      const records = await request(
        `/xrpc/com.atproto.repo.listRecords?repo=${encodeURIComponent(did())}&collection=app.bsky.actor.profile&limit=5`,
        { headers: { Authorization: `Bearer ${session.json.accessJwt}` } },
      );
      if (records.status !== 200) throw new Error(`profile listRecords expected 200, got ${records.status}: ${records.text}`);
      if (!records.json?.records?.some((item) => item.uri === `at://${did()}/app.bsky.actor.profile/self`)) {
        throw new Error("profile listRecords missing self record");
      }

      const profile = await request(`/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did())}`);
      if (profile.status !== 200) throw new Error(`profile after expected 200, got ${profile.status}: ${profile.text}`);
      if (profile.json?.displayName !== displayName || profile.json?.description !== description) {
        throw new Error("AppView profile did not reflect profile record write");
      }
    } finally {
      const restore = await writeProfile(previousDisplayName, previousDescription);
      if (restore.status !== 200) {
        throw new Error(`profile restore expected 200, got ${restore.status}: ${restore.text}`);
      }
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

  requirement("BLUESKY-MODERATION-01", "Moderation and preference probes return private-safe shapes", async () => {
    const unauthPreferences = await request("/xrpc/app.bsky.actor.getPreferences");
    const unauthBlocks = await request("/xrpc/app.bsky.graph.getBlocks");
    const unauthMutes = await request("/xrpc/app.bsky.graph.getMutes");
    if ([unauthPreferences, unauthBlocks, unauthMutes].some((res) => res.status !== 401)) {
      throw new Error(
        `auth-protected moderation endpoints expected 401s, got ${[
          unauthPreferences.status,
          unauthBlocks.status,
          unauthMutes.status,
        ].join("/")}`,
      );
    }

    const labelers = await request(`/xrpc/app.bsky.labeler.getServices?dids=${encodeURIComponent(did())}`);
    if (labelers.status !== 200) throw new Error(`labeler getServices expected 200, got ${labelers.status}: ${labelers.text}`);
    expectArray(labelers.json?.views, "labeler views");

    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for authenticated moderation probes" };
    }
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const auth = { Authorization: `Bearer ${session.json.accessJwt}` };
    const preferences = await request("/xrpc/app.bsky.actor.getPreferences", { headers: auth });
    const blocks = await request("/xrpc/app.bsky.graph.getBlocks?limit=5", { headers: auth });
    const mutes = await request("/xrpc/app.bsky.graph.getMutes?limit=5", { headers: auth });
    const statuses = [preferences, blocks, mutes].map((res) => res.status);
    if (statuses.some((status) => status !== 200)) throw new Error(`authenticated moderation endpoints expected 200s, got ${statuses.join("/")}`);
    expectArray(preferences.json?.preferences, "preferences");
    expectArray(blocks.json?.blocks, "blocks");
    expectArray(mutes.json?.mutes, "mutes");
    if (!preferences.json.preferences.some((pref) => pref.$type === "app.bsky.actor.defs#adultContentPref")) {
      throw new Error("preferences missing adultContentPref default");
    }
  }),

  requirement("BLUESKY-SEARCH-01", "AppView search endpoints return public post and actor result arrays", async () => {
    const posts = await request("/xrpc/app.bsky.feed.searchPosts?q=dais&limit=5");
    const actors = await request(`/xrpc/app.bsky.actor.searchActors?q=${encodeURIComponent(config.acctDomain)}&limit=5`);
    const typeahead = await request("/xrpc/app.bsky.actor.searchActorsTypeahead?q=dais&limit=5");
    const statuses = [posts, actors, typeahead].map((res) => res.status);
    if (statuses.some((value) => value !== 200)) throw new Error(`expected all 200, got ${statuses.join("/")}`);
    expectArray(posts.json?.posts, "search posts");
    expectArray(actors.json?.actors, "search actors");
    expectArray(typeahead.json?.actors, "typeahead actors");
    for (const post of posts.json.posts) {
      assertPublicPostShape(post);
    }
    if (!actors.json.actors.some((actor) => actor.did === did())) throw new Error("actor search missing local profile");
    if (!typeahead.json.actors.some((actor) => actor.did === did())) throw new Error("actor typeahead missing local profile");
  }),

  requirement("BLUESKY-BLOB-01", "Public image embeds expose downloadable com.atproto.sync.getBlob bytes", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for media fixture" };
    }

    let createdId = "";
    const token = `dais Bluesky blob conformance ${new Date().toISOString()}`;
    try {
      const media = await socialRequest("/api/v1/media", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          filename: "dais-bluesky-blob.png",
          media_type: "image/png",
          data_base64: tinyPngBase64,
          description: "dais Bluesky blob conformance image",
        }),
      });
      if (media.status !== 200) throw new Error(`media create expected 200, got ${media.status}: ${media.text}`);

      const status = await socialRequest("/api/v1/statuses", {
        method: "POST",
        auth: true,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          status: token,
          visibility: "public",
          media_ids: [media.json.id],
        }),
      });
      if (status.status !== 201) throw new Error(`status create expected 201, got ${status.status}: ${status.text}`);
      createdId = status.json?.id || "";

      let imageBlob;
      for (let attempt = 0; attempt < 5; attempt += 1) {
        const search = await request(`/xrpc/app.bsky.feed.searchPosts?q=${encodeURIComponent(token)}&limit=5`);
        if (search.status !== 200) throw new Error(`search expected 200, got ${search.status}: ${search.text}`);
        const post = search.json?.posts?.find((item) => item.record?.text === token);
        imageBlob = post?.record?.embed?.images?.[0]?.image;
        if (imageBlob?.ref?.$link) break;
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      if (!imageBlob?.ref?.$link) throw new Error("search result did not expose an image blob ref");
      if (imageBlob.mimeType !== "image/png") throw new Error(`unexpected blob mimeType ${imageBlob.mimeType}`);

      const blob = await request(
        `/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(did())}&cid=${encodeURIComponent(imageBlob.ref.$link)}`,
        { headers: { Accept: "image/png" } },
      );
      if (blob.status !== 200) throw new Error(`getBlob expected 200, got ${blob.status}: ${blob.text}`);
      if (!blob.contentType.includes("image/png")) throw new Error(`getBlob content-type mismatch: ${blob.contentType}`);
      if (blob.text.length === 0) throw new Error("getBlob returned empty body");

      const blobs = await request(`/xrpc/com.atproto.sync.listBlobs?did=${encodeURIComponent(did())}&limit=20`);
      if (blobs.status !== 200) throw new Error(`listBlobs expected 200, got ${blobs.status}: ${blobs.text}`);
      if (!blobs.json?.cids?.includes(imageBlob.ref.$link)) throw new Error("listBlobs did not include fixture image CID");
    } finally {
      if (createdId) {
        await socialRequest(`/api/v1/statuses/${encodeURIComponent(createdId)}`, { method: "DELETE", auth: true });
      }
    }
  }),

  requirement("BLUESKY-WRITE-01", "Owner-token ATProto session can create and delete a public feed post", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for write fixture" };
    }

    let createdUri = "";
    let createdRkey = "";
    const token = `dais Bluesky write conformance ${new Date().toISOString()}`;
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    if (session.json?.did !== did() || session.json?.handle !== config.acctDomain || !session.json?.accessJwt) {
      throw new Error("createSession shape mismatch");
    }

    try {
      const create = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${session.json.accessJwt}`,
        },
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: token,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (create.status !== 200) throw new Error(`createRecord expected 200, got ${create.status}: ${create.text}`);
      createdUri = create.json?.uri || "";
      createdRkey = rkeyFromAtUri(createdUri);
      if (!createdUri.startsWith(`at://${did()}/app.bsky.feed.post/`) || !create.json?.cid) {
        throw new Error("createRecord shape mismatch");
      }

      const record = await request(
        `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(createdRkey)}`,
      );
      if (record.status !== 200) throw new Error(`getRecord after create expected 200, got ${record.status}: ${record.text}`);
      if (record.json?.value?.text !== token) throw new Error("created record text did not round-trip");
    } finally {
      if (createdRkey) {
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${session.json.accessJwt}`,
          },
          body: JSON.stringify({
            repo: did(),
            collection: "app.bsky.feed.post",
            rkey: createdRkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-RECORD-SHAPE-01", "feed.post records expose facets, tags, language, and self-label metadata", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for feed.post shape fixture" };
    }

    let createdRkey = "";
    const stamp = new Date().toISOString();
    const tag = `DaisFacet${Date.now()}`;
    const url = "https://example.com/dais-facet";
    const text = `dais Bluesky record shape ${stamp} ${url} #${tag}`;
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };

    try {
      const create = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text,
            createdAt: stamp,
            labels: {
              $type: "com.atproto.label.defs#selfLabels",
              values: [{ val: "!warn" }],
            },
          },
        }),
      });
      if (create.status !== 200) throw new Error(`createRecord expected 200, got ${create.status}: ${create.text}`);
      createdRkey = rkeyFromAtUri(create.json?.uri);

      const record = await request(
        `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(createdRkey)}`,
      );
      if (record.status !== 200) throw new Error(`getRecord expected 200, got ${record.status}: ${record.text}`);
      const value = record.json?.value || {};
      if (value.text !== text) throw new Error("feed.post text did not round-trip");
      if (!value.langs?.includes("en")) throw new Error("feed.post missing default language metadata");
      if (!value.tags?.includes(tag)) throw new Error("feed.post missing tag metadata");
      if (value.labels?.$type !== "com.atproto.label.defs#selfLabels") throw new Error("feed.post labels type mismatch");
      if (!value.labels?.values?.some((label) => label.val === "!warn")) throw new Error("feed.post missing !warn self-label");

      const facets = value.facets || [];
      const linkFacet = facets.find((facet) => facet.features?.some((feature) => feature.$type === "app.bsky.richtext.facet#link" && feature.uri === url));
      const tagFacet = facets.find((facet) => facet.features?.some((feature) => feature.$type === "app.bsky.richtext.facet#tag" && feature.tag === tag));
      if (!linkFacet) throw new Error("feed.post missing URL richtext facet");
      if (!tagFacet) throw new Error("feed.post missing hashtag richtext facet");
      if (text.slice(linkFacet.index.byteStart, linkFacet.index.byteEnd) !== url) throw new Error("URL facet byte range mismatch");
      if (text.slice(tagFacet.index.byteStart, tagFacet.index.byteEnd) !== `#${tag}`) throw new Error("hashtag facet byte range mismatch");

      const search = await request(`/xrpc/app.bsky.feed.searchPosts?q=${encodeURIComponent(tag)}&limit=5`);
      if (search.status !== 200) throw new Error(`search expected 200, got ${search.status}: ${search.text}`);
      const post = search.json?.posts?.find((item) => item.uri === create.json?.uri);
      if (!post?.record?.facets?.length) throw new Error("AppView search result did not expose record facets");
    } finally {
      if (createdRkey) {
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: "app.bsky.feed.post",
            rkey: createdRkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-REPLY-01", "Owner-token ATProto feed replies preserve root and parent refs", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for reply fixture" };
    }

    const token = `dais Bluesky reply conformance ${new Date().toISOString()}`;
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const createdRkeys = [];
    try {
      const parent = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: `${token} parent`,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (parent.status !== 200) throw new Error(`parent createRecord expected 200, got ${parent.status}: ${parent.text}`);
      const parentRef = { uri: parent.json?.uri, cid: parent.json?.cid };
      if (!parentRef.uri || !parentRef.cid) throw new Error("parent createRecord missing uri/cid");
      createdRkeys.push(rkeyFromAtUri(parentRef.uri));

      const reply = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: `${token} child`,
            createdAt: new Date().toISOString(),
            reply: {
              root: parentRef,
              parent: parentRef,
            },
          },
        }),
      });
      if (reply.status !== 200) throw new Error(`reply createRecord expected 200, got ${reply.status}: ${reply.text}`);
      const replyRkey = rkeyFromAtUri(reply.json?.uri);
      createdRkeys.push(replyRkey);

      const record = await request(
        `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(replyRkey)}`,
      );
      if (record.status !== 200) throw new Error(`reply getRecord expected 200, got ${record.status}: ${record.text}`);
      const replyValue = record.json?.value?.reply;
      if (replyValue?.parent?.uri !== parentRef.uri || replyValue?.parent?.cid !== parentRef.cid) {
        throw new Error("reply parent ref did not round-trip");
      }
      if (replyValue?.root?.uri !== parentRef.uri || replyValue?.root?.cid !== parentRef.cid) {
        throw new Error("reply root ref did not round-trip");
      }
    } finally {
      for (const rkey of createdRkeys.reverse()) {
        if (!rkey) continue;
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: "app.bsky.feed.post",
            rkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-UPLOAD-01", "Owner-token uploadBlob can attach a public image to a feed post", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for upload fixture" };
    }

    let createdRkey = "";
    const token = `dais Bluesky upload conformance ${new Date().toISOString()}`;
    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    try {
      const upload = await request("/xrpc/com.atproto.repo.uploadBlob", {
        method: "POST",
        headers: {
          "Content-Type": "image/png",
          Authorization: `Bearer ${session.json.accessJwt}`,
        },
        body: Buffer.from(tinyPngBase64, "base64"),
      });
      if (upload.status !== 200) throw new Error(`uploadBlob expected 200, got ${upload.status}: ${upload.text}`);
      const blob = upload.json?.blob;
      if (blob?.mimeType !== "image/png" || !blob?.ref?.$link) throw new Error("uploadBlob shape mismatch");

      const create = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: token,
            createdAt: new Date().toISOString(),
            embed: {
              $type: "app.bsky.embed.images",
              images: [{ alt: "dais upload conformance", image: blob }],
            },
          },
        }),
      });
      if (create.status !== 200) throw new Error(`createRecord expected 200, got ${create.status}: ${create.text}`);
      createdRkey = rkeyFromAtUri(create.json?.uri);

      const record = await request(
        `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.post&rkey=${encodeURIComponent(createdRkey)}`,
      );
      if (record.status !== 200) throw new Error(`getRecord expected 200, got ${record.status}: ${record.text}`);
      const image = record.json?.value?.embed?.images?.[0]?.image;
      if (image?.ref?.$link !== blob.ref.$link) throw new Error("created image blob ref did not round-trip");
      if (!image?.size) throw new Error("created image blob size did not round-trip");

      const downloaded = await request(
        `/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(did())}&cid=${encodeURIComponent(blob.ref.$link)}`,
        { headers: { Accept: "image/png" } },
      );
      if (downloaded.status !== 200) throw new Error(`getBlob expected 200, got ${downloaded.status}: ${downloaded.text}`);
      if (downloaded.text.length === 0) throw new Error("getBlob returned empty uploaded body");
    } finally {
      if (createdRkey) {
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: "app.bsky.feed.post",
            rkey: createdRkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-SOCIAL-WRITE-01", "Owner-token ATProto like, repost, and follow records round-trip", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for social write fixture" };
    }

    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const timeline = await request("/xrpc/app.bsky.feed.getTimeline?limit=1");
    const subject = timeline.json?.feed?.[0]?.post;
    if (!subject?.uri || !subject?.cid) throw new Error("no public post subject available");

    const created = [];
    const followDid = "did:web:example.com";
    try {
      for (const collection of ["app.bsky.feed.like", "app.bsky.feed.repost"]) {
        const create = await request("/xrpc/com.atproto.repo.createRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection,
            record: {
              $type: collection,
              subject: { uri: subject.uri, cid: subject.cid },
              createdAt: new Date().toISOString(),
            },
          }),
        });
        if (create.status !== 200) throw new Error(`${collection} create expected 200, got ${create.status}: ${create.text}`);
        created.push({ collection, rkey: rkeyFromAtUri(create.json.uri), subject: subject.uri });
      }

      const follow = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.graph.follow",
          record: {
            $type: "app.bsky.graph.follow",
            subject: followDid,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (follow.status !== 200) throw new Error(`follow create expected 200, got ${follow.status}: ${follow.text}`);
      created.push({ collection: "app.bsky.graph.follow", rkey: rkeyFromAtUri(follow.json.uri), subject: followDid });

      for (const item of created) {
        const record = await request(
          `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=${encodeURIComponent(item.collection)}&rkey=${encodeURIComponent(item.rkey)}`,
          { headers: { Authorization: `Bearer ${session.json.accessJwt}` } },
        );
        if (record.status !== 200) throw new Error(`${item.collection} getRecord expected 200, got ${record.status}: ${record.text}`);
        if (record.json?.value?.$type !== item.collection) throw new Error(`${item.collection} getRecord type mismatch`);
        if (item.collection === "app.bsky.graph.follow") {
          if (record.json?.value?.subject !== item.subject) throw new Error("follow getRecord subject mismatch");
        } else if (record.json?.value?.subject?.uri !== item.subject) {
          throw new Error(`${item.collection} getRecord subject mismatch`);
        }

        const listed = await request(
          `/xrpc/com.atproto.repo.listRecords?repo=${encodeURIComponent(did())}&collection=${encodeURIComponent(item.collection)}&limit=20`,
          { headers: { Authorization: `Bearer ${session.json.accessJwt}` } },
        );
        if (listed.status !== 200) throw new Error(`${item.collection} listRecords expected 200, got ${listed.status}: ${listed.text}`);
        const found = listed.json?.records?.some((record) => {
          const value = record.value || {};
          if (item.collection === "app.bsky.graph.follow") return value.subject === item.subject;
          return value.subject?.uri === item.subject;
        });
        if (!found) throw new Error(`${item.collection} listRecords did not include created subject`);
      }
    } finally {
      for (const item of created.reverse()) {
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: item.collection,
            rkey: item.rkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-APPVIEW-COUNTS-01", "AppView post views expose reply, repost, and like counts", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for AppView count fixture" };
    }

    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const token = `dais Bluesky count conformance ${new Date().toISOString()}`;
    const created = [];
    try {
      const parent = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: token,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (parent.status !== 200) throw new Error(`parent createRecord expected 200, got ${parent.status}: ${parent.text}`);
      const parentRef = { uri: parent.json?.uri, cid: parent.json?.cid };
      if (!parentRef.uri || !parentRef.cid) throw new Error("parent createRecord missing uri/cid");
      created.push({ collection: "app.bsky.feed.post", rkey: rkeyFromAtUri(parentRef.uri) });

      for (const collection of ["app.bsky.feed.like", "app.bsky.feed.repost"]) {
        const create = await request("/xrpc/com.atproto.repo.createRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection,
            record: {
              $type: collection,
              subject: parentRef,
              createdAt: new Date().toISOString(),
            },
          }),
        });
        if (create.status !== 200) throw new Error(`${collection} create expected 200, got ${create.status}: ${create.text}`);
        created.push({ collection, rkey: rkeyFromAtUri(create.json?.uri) });
      }

      const reply = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: `${token} reply`,
            createdAt: new Date().toISOString(),
            reply: {
              root: parentRef,
              parent: parentRef,
            },
          },
        }),
      });
      if (reply.status !== 200) throw new Error(`reply createRecord expected 200, got ${reply.status}: ${reply.text}`);
      created.push({ collection: "app.bsky.feed.post", rkey: rkeyFromAtUri(reply.json?.uri) });

      let counted;
      for (let attempt = 0; attempt < 5; attempt += 1) {
        const search = await request(`/xrpc/app.bsky.feed.searchPosts?q=${encodeURIComponent(token)}&limit=10`);
        if (search.status !== 200) throw new Error(`search expected 200, got ${search.status}: ${search.text}`);
        counted = search.json?.posts?.find((post) => post.uri === parentRef.uri);
        if (counted?.likeCount >= 1 && counted?.repostCount >= 1 && counted?.replyCount >= 1) break;
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      if (!counted) throw new Error("count fixture parent post was not visible in AppView search");
      if (counted.likeCount < 1) throw new Error(`likeCount did not include fixture like: ${counted.likeCount}`);
      if (counted.repostCount < 1) throw new Error(`repostCount did not include fixture repost: ${counted.repostCount}`);
      if (counted.replyCount < 1) throw new Error(`replyCount did not include fixture reply: ${counted.replyCount}`);
    } finally {
      for (const item of created.reverse()) {
        if (!item.rkey) continue;
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: item.collection,
            rkey: item.rkey,
          }),
        });
      }
    }
  }),

  requirement("BLUESKY-THREAD-01", "AppView getPostThread returns public post replies", async () => {
    if (!config.mastodonToken) {
      return { status: "SKIP", detail: "set DAIS_MASTODON_BEARER_TOKEN for thread fixture" };
    }

    const session = await request("/xrpc/com.atproto.server.createSession", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ identifier: did(), password: config.mastodonToken }),
    });
    if (session.status !== 200) throw new Error(`createSession expected 200, got ${session.status}: ${session.text}`);
    const authHeaders = {
      "Content-Type": "application/json",
      Authorization: `Bearer ${session.json.accessJwt}`,
    };
    const token = `dais Bluesky thread conformance ${new Date().toISOString()}`;
    const created = [];
    try {
      const parent = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: token,
            createdAt: new Date().toISOString(),
          },
        }),
      });
      if (parent.status !== 200) throw new Error(`parent createRecord expected 200, got ${parent.status}: ${parent.text}`);
      const parentRef = { uri: parent.json?.uri, cid: parent.json?.cid };
      if (!parentRef.uri || !parentRef.cid) throw new Error("parent createRecord missing uri/cid");
      created.push({ collection: "app.bsky.feed.post", rkey: rkeyFromAtUri(parentRef.uri) });

      const reply = await request("/xrpc/com.atproto.repo.createRecord", {
        method: "POST",
        headers: authHeaders,
        body: JSON.stringify({
          repo: did(),
          collection: "app.bsky.feed.post",
          record: {
            $type: "app.bsky.feed.post",
            text: `${token} reply`,
            createdAt: new Date().toISOString(),
            reply: {
              root: parentRef,
              parent: parentRef,
            },
          },
        }),
      });
      if (reply.status !== 200) throw new Error(`reply createRecord expected 200, got ${reply.status}: ${reply.text}`);
      const replyRef = { uri: reply.json?.uri, cid: reply.json?.cid };
      if (!replyRef.uri || !replyRef.cid) throw new Error("reply createRecord missing uri/cid");
      created.push({ collection: "app.bsky.feed.post", rkey: rkeyFromAtUri(replyRef.uri) });

      let thread;
      for (let attempt = 0; attempt < 5; attempt += 1) {
        const res = await request(`/xrpc/app.bsky.feed.getPostThread?uri=${encodeURIComponent(parentRef.uri)}&depth=2`);
        if (res.status !== 200) throw new Error(`getPostThread expected 200, got ${res.status}: ${res.text}`);
        thread = res.json?.thread;
        if (thread?.replies?.some((item) => item.post?.uri === replyRef.uri)) break;
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      if (thread?.post?.uri !== parentRef.uri) throw new Error("thread parent post URI mismatch");
      if (!Array.isArray(thread?.replies)) throw new Error("thread replies was not an array");
      if (!thread.replies.some((item) => item.post?.uri === replyRef.uri)) {
        throw new Error("thread replies did not include fixture reply");
      }
      if (thread.post.replyCount < 1) throw new Error(`thread parent replyCount did not include fixture reply: ${thread.post.replyCount}`);

      const missing = await request(
        `/xrpc/app.bsky.feed.getPostThread?uri=${encodeURIComponent(`at://${did()}/app.bsky.feed.post/missing-thread-fixture`)}`,
      );
      if (missing.status !== 200) throw new Error(`missing getPostThread expected 200, got ${missing.status}: ${missing.text}`);
      if (missing.json?.thread?.notFound !== true) throw new Error("missing getPostThread did not return notFound");
    } finally {
      for (const item of created.reverse()) {
        if (!item.rkey) continue;
        await request("/xrpc/com.atproto.repo.deleteRecord", {
          method: "POST",
          headers: authHeaders,
          body: JSON.stringify({
            repo: did(),
            collection: item.collection,
            rkey: item.rkey,
          }),
        });
      }
    }
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

  requirement("BLUESKY-SYNC-02", "subscribeRepos WebSocket emits repo commit snapshot", async () => {
    const messages = await subscribeReposMessages();
    const info = messages.find((message) => message.t === "#info");
    const commit = messages.find((message) => message.t === "#commit")?.commit;
    if (info?.info?.name !== "dais-pds") throw new Error("subscribeRepos info frame shape mismatch");
    if (commit?.repo !== did()) throw new Error(`subscribeRepos commit repo mismatch: ${commit?.repo}`);
    if (!Number.isInteger(commit?.seq) || commit.seq < 1) throw new Error("subscribeRepos commit missing integer seq");
    if (!commit?.commit?.$link || !commit?.rev) throw new Error("subscribeRepos commit missing commit/rev");
    expectArray(commit?.ops, "subscribeRepos commit ops");
    if (!commit.ops.some((op) => op.path === "app.bsky.actor.profile/self")) {
      throw new Error("subscribeRepos commit missing profile op");
    }
  }),

  requirement("BLUESKY-ERROR-01", "Unsupported repo collections fail explicitly", async () => {
    const res = await request(
      `/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did())}&collection=app.bsky.feed.threadgate&rkey=missing`,
    );
    if (res.status !== 404) throw new Error(`expected 404 for unsupported collection, got ${res.status}`);
  }),
];

for (const test of tests) {
  try {
    const outcome = await test.run();
    if (outcome?.status === "SKIP") {
      record(test, "SKIP", outcome.detail || "");
    } else {
      record(test, "PASS");
    }
  } catch (error) {
    record(test, "FAIL", error.message || String(error));
  }
}

let pass = 0;
let fail = 0;
let skip = 0;
console.log("\nBluesky compatibility report");
console.log(`Target: ${config.pdsBaseUrl}`);
for (const result of results) {
  if (result.status === "PASS") pass += 1;
  if (result.status === "FAIL") fail += 1;
  if (result.status === "SKIP") skip += 1;
  console.log(`${result.status.padEnd(5)} ${result.id.padEnd(18)} ${result.title}${result.detail ? ` - ${result.detail}` : ""}`);
}
console.log(`\nSummary: PASS=${pass} FAIL=${fail} SKIP=${skip}`);
if (fail > 0) process.exit(1);
