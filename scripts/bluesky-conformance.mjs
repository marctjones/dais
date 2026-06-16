#!/usr/bin/env node

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
