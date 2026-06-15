import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Visibility = "Public" | "Unlisted" | "Followers" | "Direct";
type ProtocolRoute = "ActivityPub" | "AtProto" | "Both";

type OwnerSnapshot = {
  settings: {
    instance_url: string;
    owner_token_present: boolean;
    default_visibility: Visibility;
    default_protocol: ProtocolRoute;
  };
  profile: {
    id: string;
    username: string;
    actor_type: string;
    display_name?: string | null;
    summary?: string | null;
    icon?: string | null;
    image?: string | null;
    avatar_url?: string | null;
    header_url?: string | null;
    public_handle: string;
    actor_url: string;
  };
  active_section: string;
  home_timeline: Array<{
    id: string;
    object_id: string;
    actor_id: string;
    actor_username?: string | null;
    actor_display_name?: string | null;
    actor_avatar_url?: string | null;
    content: string;
    content_html?: string | null;
    visibility: string;
    in_reply_to?: string | null;
    published_at?: string | null;
    protocol?: string | null;
    reply_count?: number;
    like_count?: number;
    boost_count?: number;
  }>;
  posts: Array<{
    id: string;
    title?: string | null;
    content: string;
    visibility: Visibility | string;
    protocol: ProtocolRoute | string;
    encrypted: boolean;
    attachments?: Array<{ url?: string; mediaType?: string; name?: string }>;
    reply_count?: number;
    like_count?: number;
    boost_count?: number;
    published_at?: string | null;
  }>;
  followers: Array<{
    id: string;
    actor_id: string;
    follower_actor_id: string;
    follower_inbox: string;
    follower_shared_inbox?: string | null;
    status: string;
    created_at?: string | null;
    updated_at?: string | null;
  }>;
  friends: Array<{
    friend_actor_id: string;
    friend_inbox?: string | null;
    friend_shared_inbox?: string | null;
    follower_since?: string | null;
    following_since?: string | null;
    accepted_at?: string | null;
  }>;
  following: Array<{
    id: string;
    actor_id: string;
    target_actor_id: string;
    target_inbox: string;
    status: string;
    created_at?: string | null;
    accepted_at?: string | null;
  }>;
  sources: Array<{
    id: string;
    title: string;
    source_type: string;
    canonical_url?: string | null;
    excerpt?: string | null;
    read: boolean;
  }>;
  moderation: ModerationState;
  diagnostics: Array<{ key: string; ok: boolean; detail: string }>;
};

type ModerationState = {
  closed_network: boolean;
  block_count: number;
  allowlist_count: number;
  blocks?: ModerationBlock[];
  allowlist?: ModerationAllowlistHost[];
};

type ModerationBlock = {
  id: string;
  actor_id: string;
  blocked_domain?: string | null;
  reason?: string | null;
  created_at?: string | null;
};

type ModerationAllowlistHost = {
  host: string;
  note?: string | null;
  enabled: boolean | number | string | null;
  created_at?: string | null;
  updated_at?: string | null;
};

type CreatedPost = {
  id: string;
  visibility: string;
  protocol: string;
  in_reply_to?: string | null;
  published_at: string;
};

type FollowResult = {
  ok: boolean;
  following: OwnerSnapshot["following"][number];
  delivery_ids: string[];
};

type DiscoveredActor = {
  id: string;
  inbox: string;
  shared_inbox?: string | null;
  preferred_username?: string | null;
  name?: string | null;
  summary?: string | null;
  url?: string | null;
  icon_url?: string | null;
  handle?: string | null;
  following_status?: string | null;
};

type InteractionResult = {
  ok: boolean;
  activity_id: string;
  interaction: string;
  object_id: string;
  delivery_ids: string[];
};

type UploadedMedia = {
  url: string;
  media_type?: string | null;
  access?: string | null;
  attachment: unknown;
};

type OwnerPostDetail = OwnerSnapshot["posts"][number] & {
  content_html?: string | null;
  in_reply_to?: string | null;
  replies?: PostReply[];
  likes?: PostInteraction[];
  boosts?: PostInteraction[];
};

type PostReply = {
  id: string;
  actor_id: string;
  actor_username?: string | null;
  actor_display_name?: string | null;
  content?: string | null;
  published_at?: string | null;
  created_at?: string | null;
};

type PostInteraction = {
  id: string;
  actor_id: string;
  actor_username?: string | null;
  actor_display_name?: string | null;
  object_url?: string | null;
  created_at?: string | null;
};

type OwnerNotification = {
  id: string;
  type: string;
  actor_id: string;
  actor_username?: string | null;
  actor_display_name?: string | null;
  actor_avatar_url?: string | null;
  post_id?: string | null;
  activity_id?: string | null;
  content?: string | null;
  read: boolean | number | string | null;
  created_at?: string | null;
};

type OwnerDelivery = {
  id: string;
  post_id: string;
  target_type?: string | null;
  target_url: string;
  protocol: string;
  status: string;
  retry_count?: number | null;
  last_attempt_at?: string | null;
  error_message?: string | null;
  activity_type?: string | null;
  created_at?: string | null;
  delivered_at?: string | null;
};

type SourceSubscription = {
  id: string;
  source_type: string;
  url: string;
  title?: string | null;
  homepage_url?: string | null;
  status: string;
  refresh_cadence_minutes: number;
  last_fetched_at?: string | null;
  next_fetch_at?: string | null;
  last_error?: string | null;
  error_count: number;
  policy_json: string;
  created_at?: string | null;
  updated_at?: string | null;
};

type OwnerSources = {
  subscriptions: SourceSubscription[];
  items: OwnerSnapshot["sources"];
};

const sections = [
  "Home",
  "Following",
  "Discovery",
  "Compose",
  "Posts",
  "Sources",
  "Notifications",
  "Followers",
  "Profile",
  "Moderation",
  "Deliveries",
  "Settings",
  "Diagnostics"
];

let snapshot: OwnerSnapshot | null = null;
let active = "Home";
let notice = "";
let draftAttachments: string[] = [];
let draftReplyTo = "";
let discoveredActor: DiscoveredActor | null = null;
let selectedPostDetail: OwnerPostDetail | null = null;
let notifications: OwnerNotification[] = [];
let deliveries: OwnerDelivery[] = [];
let sourceSubscriptions: SourceSubscription[] = [];
let sourceItems: OwnerSnapshot["sources"] = [];
let moderationState: ModerationState | null = null;

async function load() {
  render();
  snapshot = await invoke<OwnerSnapshot>("owner_snapshot");
  active = active || snapshot.active_section || "Home";
  render();
  await loadLiveSection(active);
}

function render() {
  const app = document.querySelector<HTMLDivElement>("#app");
  if (!app) return;
  if (!snapshot) {
    app.innerHTML = `<main class="loading">Loading dais owner...</main>`;
    return;
  }

  const apiDiagnostic = snapshot.diagnostics.find((row) => row.key === "owner-api");
  app.innerHTML = `
    <main class="shell">
      <aside class="sidebar">
        <div class="brand">
          <div class="mark">d</div>
          <div>
            <strong>dais owner</strong>
            <span>${escapeHtml(shortHost(snapshot.settings.instance_url))}</span>
          </div>
        </div>
        <nav class="nav">${sections.map(navButton).join("")}</nav>
      </aside>
      <section class="workspace">
        <header class="topbar">
          <div>
            <h1>${escapeHtml(active)}</h1>
            <p>${escapeHtml(sectionSubtitle(active))}</p>
          </div>
          <div class="top-actions">
            <span class="pill ${snapshot.settings.owner_token_present ? "ok" : "warn"}">${
              snapshot.settings.owner_token_present ? "Token stored" : "Token needed"
            }</span>
            <span class="pill ${apiDiagnostic?.ok ? "ok" : "warn"}">${
              apiDiagnostic?.ok ? "Owner API live" : "Local preview"
            }</span>
          </div>
        </header>
        ${notice ? `<div class="notice">${escapeHtml(notice)}</div>` : ""}
        ${view(active, snapshot)}
      </section>
    </main>`;

  app.querySelectorAll<HTMLButtonElement>("[data-section]").forEach((button) => {
    button.addEventListener("click", () => {
      active = button.dataset.section || "Home";
      notice = "";
      render();
      void loadLiveSection(active);
    });
  });
  app.querySelector<HTMLFormElement>("#settings-form")?.addEventListener("submit", saveSettings);
  app.querySelector<HTMLFormElement>("#profile-form")?.addEventListener("submit", saveProfile);
  app.querySelector<HTMLFormElement>("#compose-form")?.addEventListener("submit", publishPost);
  app.querySelector<HTMLButtonElement>("[data-clear-reply]")?.addEventListener("click", () => {
    draftReplyTo = "";
    render();
  });
  app.querySelector<HTMLButtonElement>("#media-upload")?.addEventListener("click", uploadSelectedMedia);
  app.querySelectorAll<HTMLButtonElement>("[data-remove-attachment]").forEach((button) => {
    button.addEventListener("click", () => {
      draftAttachments = draftAttachments.filter((url) => url !== button.dataset.removeAttachment);
      render();
    });
  });
  app.querySelector<HTMLFormElement>("#follow-form")?.addEventListener("submit", followActor);
  app.querySelector<HTMLFormElement>("#discover-form")?.addEventListener("submit", discoverActor);
  app.querySelector<HTMLFormElement>("#source-form")?.addEventListener("submit", addSource);
  app.querySelector<HTMLFormElement>("#block-actor-form")?.addEventListener("submit", blockActor);
  app.querySelector<HTMLFormElement>("#block-domain-form")?.addEventListener("submit", blockDomain);
  app.querySelector<HTMLFormElement>("#allow-host-form")?.addEventListener("submit", allowHost);
  app.querySelector<HTMLButtonElement>("[data-refresh-sources]")?.addEventListener("click", () => {
    void refreshSource(null);
  });
  app.querySelector<HTMLButtonElement>("[data-follow-discovered]")?.addEventListener("click", () => {
    void followTarget(discoveredActor?.id || "");
  });
  app.querySelectorAll<HTMLButtonElement>("[data-follower-status]").forEach((button) => {
    button.addEventListener("click", () => {
      const follower = button.dataset.follower || "";
      const status = button.dataset.followerStatus || "";
      void setFollowerStatus(follower, status);
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-unfollow]").forEach((button) => {
    button.addEventListener("click", () => {
      void unfollowActor(button.dataset.unfollow || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-timeline-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const objectId = button.dataset.object || "";
      const action = button.dataset.timelineAction || "";
      if (action === "reply") {
        draftReplyTo = objectId;
        active = "Compose";
        notice = `Replying to ${shortUrl(objectId)}.`;
        render();
      } else {
        void ownerInteraction(objectId, action);
      }
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-post-detail]").forEach((button) => {
    button.addEventListener("click", () => {
      void loadPostDetail(button.dataset.postDetail || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-copy-link]").forEach((button) => {
    button.addEventListener("click", () => {
      void copyLink(button.dataset.copyLink || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-open-link]").forEach((button) => {
    button.addEventListener("click", () => {
      openLink(button.dataset.openLink || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-notification-read]").forEach((button) => {
    button.addEventListener("click", () => {
      void markNotificationRead(button.dataset.notificationRead || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-source-refresh]").forEach((button) => {
    button.addEventListener("click", () => {
      void refreshSource(button.dataset.sourceRefresh || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-source-remove]").forEach((button) => {
    button.addEventListener("click", () => {
      void removeSource(button.dataset.sourceRemove || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-unblock-value]").forEach((button) => {
    button.addEventListener("click", () => {
      void unblockValue(button.dataset.unblockValue || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-disallow-host]").forEach((button) => {
    button.addEventListener("click", () => {
      void disallowHost(button.dataset.disallowHost || "");
    });
  });
}

function navButton(section: string) {
  return `<button class="${section === active ? "active" : ""}" data-section="${escapeHtml(section)}">
    <span>${navGlyph(section)}</span>${escapeHtml(section)}
  </button>`;
}

function view(section: string, data: OwnerSnapshot): string {
  switch (section) {
    case "Home":
      return dashboardView(data);
    case "Following":
      return followingView(data);
    case "Discovery":
      return discoveryView();
    case "Compose":
      return composeView(data);
    case "Posts":
      return postsView(data);
    case "Sources":
      return sourcesView(data);
    case "Moderation":
      return moderationView(data);
    case "Settings":
      return settingsView(data);
    case "Diagnostics":
      return list(data.diagnostics.map(diagnosticCard), "No diagnostics returned.");
    case "Followers":
      return followersView(data);
    case "Profile":
      return profileView(data);
    case "Notifications":
      return notificationsView();
    case "Deliveries":
      return deliveriesView();
    default:
      return pendingLiveView(section);
  }
}

function postsView(data: OwnerSnapshot) {
  return `<section class="split">
    <div>${list(data.posts.map(postCard), "No posts returned by the owner API yet.")}</div>
    <article class="panel detail">
      <h2>Post detail</h2>
      ${selectedPostDetail ? postDetailView(selectedPostDetail) : `<p>Select a post to inspect replies, reactions, boosts, and attachments.</p>`}
    </article>
  </section>`;
}

function notificationsView() {
  return `<section>
    ${list(notifications.map(notificationCard), "No notifications returned by the owner API.")}
  </section>`;
}

function deliveriesView() {
  return `<section>
    ${list(deliveries.map(deliveryCard), "No deliveries returned by the owner API.")}
  </section>`;
}

function sourcesView(data: OwnerSnapshot) {
  const items = sourceItems.length ? sourceItems : data.sources;
  return `<section class="split">
    <article>
      <div class="section-heading">
        <h2 class="section-label">Subscriptions</h2>
        <button type="button" data-refresh-sources>Refresh all</button>
      </div>
      <form id="source-form" class="compact-form">
        <select name="source_type">
          <option value="rss">RSS</option>
          <option value="atom">Atom</option>
          <option value="api">API</option>
        </select>
        <input name="url" placeholder="https://example.com/feed.xml" />
        <input name="title" placeholder="Title" />
        <input name="cadence_minutes" type="number" min="5" max="1440" value="60" />
        <button type="submit">Add</button>
      </form>
      ${list(sourceSubscriptions.map(sourceSubscriptionCard), "No source subscriptions returned by the owner API.")}
    </article>
    <article>
      <h2 class="section-label">Reader items</h2>
      ${list(items.map(sourceCard), "No source items are available yet.")}
    </article>
  </section>`;
}

function dashboardView(data: OwnerSnapshot) {
  return `
    <section class="metrics">
      <article><span>Posts</span><strong>${data.posts.length}</strong></article>
      <article><span>Followers</span><strong>${data.followers.filter((row) => row.status === "approved").length}</strong></article>
      <article><span>Friends</span><strong>${data.friends.length}</strong></article>
      <article><span>Following</span><strong>${data.following.length}</strong></article>
      <article><span>Sources</span><strong>${data.sources.length}</strong></article>
    </section>
    ${composeView(data)}
    <section class="split">
      <article class="panel">
        <h2>Following feed</h2>
        ${list(data.home_timeline.slice(0, 6).map(timelineCard), "No followed posts yet. Follow people or sources to build this feed.")}
      </article>
    </section>
    <section class="split">
      <div>${list(data.posts.slice(0, 6).map(postCard), "No recent posts.")}</div>
      <div>${list(data.diagnostics.map(diagnosticCard), "No diagnostics.")}</div>
    </section>`;
}

function followingView(data: OwnerSnapshot) {
  return `<section class="split">
    <article class="panel">
      <h2>Following feed</h2>
      ${list(data.home_timeline.map(timelineCard), "No followed posts yet. Follow an ActivityPub actor to build this feed.")}
    </article>
    <article class="panel">
      <h2>Follow actor</h2>
      <form id="follow-form" class="inline-form">
        <input name="target" placeholder="@user@example.social or https://..." />
        <button type="submit">Follow</button>
      </form>
      <h2 class="section-label">Following</h2>
      ${list(data.following.map(followingCard), "No followed actors yet.")}
    </article>
  </section>`;
}

function discoveryView() {
  return `<section class="split">
    <article class="panel">
      <h2>Find actor</h2>
      <form id="discover-form" class="inline-form">
        <input name="target" placeholder="@user@example.social or https://..." />
        <button type="submit">Lookup</button>
      </form>
    </article>
    <article class="panel">
      <h2>Actor preview</h2>
      ${discoveredActor ? discoveredActorCard(discoveredActor) : `<p>No actor selected.</p>`}
    </article>
  </section>`;
}

function composeView(data: OwnerSnapshot) {
  const approvedFollowers = data.followers.filter((row) => row.status === "approved");
  return `<form id="compose-form" class="panel compose">
    <div class="compose-head">
      <h2>New post</h2>
      <span class="pill ok">Private default</span>
    </div>
    <textarea name="text" placeholder="Write to approved followers by default"></textarea>
    ${
      draftReplyTo
        ? `<div class="reply-target">
            <span>Replying to ${escapeHtml(shortUrl(draftReplyTo))}</span>
            <button type="button" data-clear-reply="1">Clear</button>
          </div>`
        : ""
    }
    <div class="form-grid">
      <label>Visibility
        <select name="visibility">
          ${option("Followers", data.settings.default_visibility === "Followers")}
          ${option("Public", data.settings.default_visibility === "Public")}
          ${option("Unlisted", data.settings.default_visibility === "Unlisted")}
          ${option("Direct", data.settings.default_visibility === "Direct")}
        </select>
      </label>
      <label>Protocol
        <select name="protocol">
          ${option("ActivityPub", data.settings.default_protocol === "ActivityPub")}
          ${option("Both", data.settings.default_protocol === "Both")}
          ${option("AtProto", data.settings.default_protocol === "AtProto")}
        </select>
      </label>
    </div>
    <label>Recipients
      <input name="recipients" placeholder="Direct/E2EE actor URLs, comma separated" />
    </label>
    <section class="media-box">
      <div class="media-row">
        <input id="media-file" type="file" accept="image/jpeg,image/png,image/gif,image/webp,video/mp4,video/webm" />
        <button id="media-upload" type="button">Upload</button>
      </div>
      ${list(
        draftAttachments.map((url) => `<article class="attachment-chip">
          <span>${escapeHtml(shortUrl(url))}</span>
          <button type="button" data-remove-attachment="${escapeAttr(url)}">Remove</button>
        </article>`),
        "No media attached."
      )}
    </section>
    <fieldset class="recipient-picker">
      <legend>Approved followers</legend>
      ${
        approvedFollowers.length
          ? approvedFollowers.map(recipientOption).join("")
          : `<p>No approved followers are available for direct selection.</p>`
      }
    </fieldset>
    <div class="compose-actions">
      <label class="check"><input name="encrypt" type="checkbox" /> E2EE</label>
      <button type="submit">Publish</button>
    </div>
  </form>`;
}

function followersView(data: OwnerSnapshot) {
  const pending = data.followers.filter((row) => row.status === "pending");
  const approved = data.followers.filter((row) => row.status === "approved");
  const rejected = data.followers.filter((row) => row.status === "rejected");
  return `<section class="split followers">
    <div>
      <h2 class="section-label">Pending</h2>
      ${list(pending.map(followerCard), "No pending follow requests.")}
    </div>
    <div>
      <h2 class="section-label">Approved</h2>
      ${list(approved.map(followerCard), "No approved followers yet.")}
      <h2 class="section-label">Rejected</h2>
      ${list(rejected.map(followerCard), "No rejected followers.")}
    </div>
  </section>`;
}

function moderationView(data: OwnerSnapshot) {
  const moderation = moderationState || data.moderation;
  const blocks = moderation.blocks || [];
  const allowlist = moderation.allowlist || [];
  return `<section class="split">
    <article class="panel"><h2>Federation safety</h2>
      <p>Closed network: ${moderation.closed_network ? "enabled" : "disabled"}</p>
      <p>Blocked actors/domains: ${moderation.block_count}</p>
      <p>Allowed hosts: ${moderation.allowlist_count}</p>
      <form id="block-actor-form" class="inline-form">
        <input name="actor_id" placeholder="https://example.social/users/name" />
        <button type="submit">Block actor</button>
      </form>
      <form id="block-domain-form" class="inline-form">
        <input name="domain" placeholder="example.social" />
        <button type="submit">Block domain</button>
      </form>
      <h2 class="section-label">Blocks</h2>
      ${list(blocks.map(blockCard), "No actor or domain blocks.")}
    </article>
    <article class="panel"><h2>Allowlist</h2>
      <form id="allow-host-form" class="inline-form">
        <input name="host" placeholder="example.social" />
        <button type="submit">Allow host</button>
      </form>
      ${list(allowlist.map(allowlistCard), "No allowed federation hosts.")}
      <h2 class="section-label">Policy</h2>
      <p>Private posts stay off public outboxes and Bluesky routes.</p>
      <p>Public routing is explicit from compose.</p>
    </article>
  </section>`;
}

function pendingLiveView(section: string) {
  return `<article class="panel empty">
    <h2>${escapeHtml(section)} live workflow</h2>
    <p>This screen is ready for the owner API parity pass. The secure owner API foundation is being wired before adding destructive controls here.</p>
  </article>`;
}

function settingsView(data: OwnerSnapshot) {
  return `<form id="settings-form" class="panel settings">
    <label>Instance URL<input name="instance" value="${escapeAttr(data.settings.instance_url)}" /></label>
    <label>Owner token<input name="token" type="password" placeholder="${data.settings.owner_token_present ? "stored" : "required"}" /></label>
    <button>Save settings</button>
  </form>`;
}

function profileView(data: OwnerSnapshot) {
  const profile = data.profile;
  return `<form id="profile-form" class="panel settings">
    <div>
      <h2>Public account</h2>
      <p>${escapeHtml(profile.public_handle)} · ${escapeHtml(profile.actor_url)}</p>
    </div>
    <label>Display name
      <input name="display_name" value="${escapeAttr(profile.display_name || "")}" />
    </label>
    <label>Actor type
      <select name="actor_type">
        ${["Person", "Group", "Organization"].map((value) => option(value, profile.actor_type === value)).join("")}
      </select>
    </label>
    <label>Summary
      <textarea name="summary" rows="5">${escapeHtml(profile.summary || "")}</textarea>
    </label>
    <label>Avatar/icon URL
      <input name="icon" value="${escapeAttr(profile.icon || profile.avatar_url || "")}" placeholder="https://..." />
    </label>
    <label>Header image URL
      <input name="image" value="${escapeAttr(profile.image || profile.header_url || "")}" placeholder="https://..." />
    </label>
    <button>Save profile</button>
  </form>`;
}

function postCard(post: OwnerSnapshot["posts"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(post.title || post.id)}</h2>
      <p>${escapeHtml(post.content)}</p>
    </div>
    <footer>
      <span>${escapeHtml(String(post.visibility))}</span>
      <span>${escapeHtml(String(post.protocol))}</span>
      ${post.encrypted ? "<span>E2EE</span>" : ""}
      ${post.attachments?.length ? `<span>${post.attachments.length} media</span>` : ""}
      ${interactionCounts(post)}
      ${post.published_at ? `<time>${escapeHtml(formatTime(post.published_at))}</time>` : ""}
      <button type="button" data-post-detail="${escapeAttr(post.id)}">Detail</button>
    </footer>
  </article>`;
}

function postDetailView(post: OwnerPostDetail) {
  return `<div class="detail-body">
    <p>${escapeHtml(post.content)}</p>
    <div class="detail-actions">
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.id)}">Reply</button>
      <button type="button" data-timeline-action="like" data-object="${escapeAttr(post.id)}">Like</button>
      <button type="button" data-timeline-action="boost" data-object="${escapeAttr(post.id)}">Boost</button>
      <button type="button" data-copy-link="${escapeAttr(post.id)}">Copy link</button>
      <button type="button" data-open-link="${escapeAttr(post.id)}">Open original</button>
    </div>
    <dl>
      <dt>ID</dt><dd>${escapeHtml(shortUrl(post.id))}</dd>
      <dt>Visibility</dt><dd>${escapeHtml(String(post.visibility))}</dd>
      <dt>Protocol</dt><dd>${escapeHtml(String(post.protocol))}</dd>
      <dt>Reply target</dt><dd>${post.in_reply_to ? escapeHtml(shortUrl(post.in_reply_to)) : "none"}</dd>
      <dt>Published</dt><dd>${post.published_at ? escapeHtml(formatTime(post.published_at)) : ""}</dd>
      <dt>Media</dt><dd>${post.attachments?.length || 0}</dd>
      <dt>Replies</dt><dd>${post.reply_count || post.replies?.length || 0}</dd>
      <dt>Likes</dt><dd>${post.like_count || post.likes?.length || 0}</dd>
      <dt>Boosts</dt><dd>${post.boost_count || post.boosts?.length || 0}</dd>
    </dl>
    ${postDetailReplies(post.replies || [])}
    ${postDetailInteractions("Likes", post.likes || [])}
    ${postDetailInteractions("Boosts", post.boosts || [])}
  </div>`;
}

function postDetailReplies(rows: PostReply[]) {
  return `<section class="detail-list">
    <h3>Replies</h3>
    ${
      rows.length
        ? rows.map((row) => `<article>
            <strong>${escapeHtml(row.actor_display_name || row.actor_username || actorLabel(row.actor_id))}</strong>
            <p>${escapeHtml(row.content || row.id)}</p>
            ${row.published_at || row.created_at ? `<time>${escapeHtml(formatTime(row.published_at || row.created_at || ""))}</time>` : ""}
          </article>`).join("")
        : `<p>None</p>`
    }
  </section>`;
}

function postDetailInteractions(title: string, rows: PostInteraction[]) {
  return `<section class="detail-list">
    <h3>${escapeHtml(title)}</h3>
    ${
      rows.length
        ? rows.map((row) => `<article>
            <strong>${escapeHtml(row.actor_display_name || row.actor_username || actorLabel(row.actor_id))}</strong>
            <span>${escapeHtml(shortUrl(row.actor_id))}</span>
            ${row.created_at ? `<time>${escapeHtml(formatTime(row.created_at))}</time>` : ""}
          </article>`).join("")
        : `<p>None</p>`
    }
  </section>`;
}

function timelineCard(post: OwnerSnapshot["home_timeline"][number]) {
  const author = post.actor_display_name || post.actor_username || actorLabel(post.actor_id);
  return `<article class="item timeline">
    <div>
      <h2>${escapeHtml(author)}</h2>
      <p>${escapeHtml(post.content)}</p>
    </div>
    <footer>
      <span>${escapeHtml(post.visibility)}</span>
      ${post.protocol ? `<span>${escapeHtml(post.protocol)}</span>` : ""}
      ${post.in_reply_to ? "<span>reply</span>" : ""}
      ${post.published_at ? `<time>${escapeHtml(formatTime(post.published_at))}</time>` : ""}
      ${interactionCounts(post)}
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.object_id)}">Reply</button>
      <button type="button" data-timeline-action="like" data-object="${escapeAttr(post.object_id)}">Like</button>
      <button type="button" data-timeline-action="boost" data-object="${escapeAttr(post.object_id)}">Boost</button>
    </footer>
  </article>`;
}

function interactionCounts(post: { reply_count?: number; like_count?: number; boost_count?: number }) {
  const parts = [];
  if (post.reply_count) parts.push(`<span>${post.reply_count} replies</span>`);
  if (post.like_count) parts.push(`<span>${post.like_count} likes</span>`);
  if (post.boost_count) parts.push(`<span>${post.boost_count} boosts</span>`);
  return parts.join("");
}

function sourceCard(source: OwnerSnapshot["sources"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(source.title)}</h2>
      <p>${escapeHtml(source.excerpt || "")}</p>
    </div>
    <footer>
      <span>${escapeHtml(source.source_type)}</span>
      <span>${source.read ? "Read" : "Unread"}</span>
      ${source.canonical_url ? `<a href="${escapeAttr(source.canonical_url)}">${escapeHtml(shortHost(source.canonical_url))}</a>` : ""}
    </footer>
  </article>`;
}

function sourceSubscriptionCard(source: SourceSubscription) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(source.title || shortHost(source.url))}</h2>
      <p>${escapeHtml(source.url)}</p>
    </div>
    <footer>
      <span>${escapeHtml(source.source_type)}</span>
      <span>${escapeHtml(source.status)}</span>
      <span>${source.refresh_cadence_minutes} min</span>
      ${source.last_fetched_at ? `<time>${escapeHtml(formatTime(source.last_fetched_at))}</time>` : ""}
      ${source.last_error ? `<span>${escapeHtml(source.last_error)}</span>` : ""}
    </footer>
    <div class="row-actions">
      <button type="button" data-source-refresh="${escapeAttr(source.id)}">Refresh</button>
      <button type="button" data-source-remove="${escapeAttr(source.id)}">Remove</button>
    </div>
  </article>`;
}

function notificationCard(row: OwnerNotification) {
  const actor = row.actor_display_name || row.actor_username || actorLabel(row.actor_id);
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.type)} from ${escapeHtml(actor)}</h2>
      <p>${escapeHtml(row.content || row.activity_id || "")}</p>
    </div>
    <footer>
      <span>${isRead(row.read) ? "read" : "unread"}</span>
      ${row.post_id ? `<span>${escapeHtml(shortUrl(row.post_id))}</span>` : ""}
      ${row.created_at ? `<time>${escapeHtml(formatTime(row.created_at))}</time>` : ""}
      <button type="button" data-notification-read="${escapeAttr(row.id)}">Mark read</button>
    </footer>
  </article>`;
}

function deliveryCard(row: OwnerDelivery) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.status)} ${escapeHtml(row.protocol)}</h2>
      <p>${escapeHtml(shortUrl(row.target_url))}</p>
    </div>
    <footer>
      ${row.activity_type ? `<span>${escapeHtml(row.activity_type)}</span>` : ""}
      <span>retries ${row.retry_count || 0}</span>
      ${row.last_attempt_at ? `<time>${escapeHtml(formatTime(row.last_attempt_at))}</time>` : ""}
      ${row.error_message ? `<span>${escapeHtml(row.error_message)}</span>` : ""}
    </footer>
  </article>`;
}

function followerCard(row: OwnerSnapshot["followers"][number]) {
  return `<article class="panel item follower">
    <div>
      <h2>${escapeHtml(actorLabel(row.follower_actor_id))}</h2>
      <p>${escapeHtml(row.follower_actor_id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.status)}</span>
      ${row.updated_at ? `<time>${escapeHtml(formatTime(row.updated_at))}</time>` : ""}
      <button type="button" data-follower-status="approved" data-follower="${escapeAttr(row.follower_actor_id)}">Approve</button>
      <button type="button" data-follower-status="pending" data-follower="${escapeAttr(row.follower_actor_id)}">Pending</button>
      <button type="button" data-follower-status="rejected" data-follower="${escapeAttr(row.follower_actor_id)}">Reject</button>
    </footer>
  </article>`;
}

function followingCard(row: OwnerSnapshot["following"][number]) {
  return `<article class="panel item follower">
    <div>
      <h2>${escapeHtml(actorLabel(row.target_actor_id))}</h2>
      <p>${escapeHtml(row.target_actor_id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.status)}</span>
      ${row.accepted_at ? `<time>${escapeHtml(formatTime(row.accepted_at))}</time>` : ""}
      <button type="button" data-unfollow="${escapeAttr(row.target_actor_id)}">Unfollow</button>
    </footer>
  </article>`;
}

function discoveredActorCard(actor: DiscoveredActor) {
  const title = actor.name || actor.handle || actor.preferred_username || actor.id;
  const status = actor.following_status || "not-following";
  return `<article class="item actor-preview">
    <div>
      ${actor.icon_url ? `<img class="avatar" src="${escapeAttr(actor.icon_url)}" alt="" />` : ""}
      <h2>${escapeHtml(title)}</h2>
      ${actor.handle ? `<p>${escapeHtml(actor.handle)}</p>` : ""}
      ${actor.summary ? `<p>${escapeHtml(stripTags(actor.summary))}</p>` : ""}
    </div>
    <footer>
      <span>${escapeHtml(status)}</span>
      <a href="${escapeAttr(actor.url || actor.id)}">${escapeHtml(shortUrl(actor.url || actor.id))}</a>
      <span>${escapeHtml(shortUrl(actor.inbox))}</span>
      <button type="button" data-follow-discovered="1">Follow</button>
    </footer>
  </article>`;
}

function recipientOption(row: OwnerSnapshot["followers"][number]) {
  const value = row.follower_actor_id;
  return `<label class="recipient-option">
    <input type="checkbox" name="follower_recipient" value="${escapeAttr(value)}" />
    <span>${escapeHtml(actorLabel(value))}</span>
  </label>`;
}

function diagnosticCard(row: OwnerSnapshot["diagnostics"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.key)}</h2>
      <p>${escapeHtml(row.detail)}</p>
    </div>
    <footer><span class="${row.ok ? "good" : "bad"}">${row.ok ? "ok" : "needs attention"}</span></footer>
  </article>`;
}

function blockCard(row: ModerationBlock) {
  const value = row.blocked_domain || row.actor_id || row.id;
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.blocked_domain || actorLabel(row.actor_id))}</h2>
      <p>${escapeHtml(row.reason || row.actor_id || row.id)}</p>
    </div>
    <footer>
      ${row.created_at ? `<time>${escapeHtml(formatTime(row.created_at))}</time>` : ""}
      <button type="button" data-unblock-value="${escapeAttr(value)}">Unblock</button>
    </footer>
  </article>`;
}

function allowlistCard(row: ModerationAllowlistHost) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.host)}</h2>
      <p>${escapeHtml(row.note || "")}</p>
    </div>
    <footer>
      <span>${isEnabled(row.enabled) ? "enabled" : "disabled"}</span>
      ${row.updated_at ? `<time>${escapeHtml(formatTime(row.updated_at))}</time>` : ""}
      <button type="button" data-disallow-host="${escapeAttr(row.host)}">Remove</button>
    </footer>
  </article>`;
}

function list(items: string[], emptyText: string) {
  return `<section class="list">${items.length ? items.join("") : `<article class="panel empty"><p>${escapeHtml(emptyText)}</p></article>`}</section>`;
}

async function saveSettings(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.target as HTMLFormElement);
  await invoke("save_owner_settings", {
    instanceUrl: String(form.get("instance") || ""),
    ownerToken: String(form.get("token") || "")
  });
  notice = "Settings saved.";
  await load();
}

async function saveProfile(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const updated = await invoke<OwnerSnapshot["profile"]>("update_owner_profile", {
    actorType: String(form.get("actor_type") || "Person"),
    displayName: String(form.get("display_name") || ""),
    summary: String(form.get("summary") || ""),
    icon: String(form.get("icon") || ""),
    image: String(form.get("image") || "")
  });
  notice = `${updated.public_handle} profile saved.`;
  await load();
}

async function publishPost(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.target as HTMLFormElement);
  const text = String(form.get("text") || "").trim();
  if (!text) {
    notice = "Write something before publishing.";
    render();
    return;
  }
  const visibility = String(form.get("visibility") || "Followers");
  const protocol = String(form.get("protocol") || "ActivityPub");
  if (draftAttachments.length > 0 && protocol !== "ActivityPub") {
    notice = "Media attachments currently require ActivityPub routing.";
    render();
    return;
  }
  if (draftAttachments.length > 0 && form.get("encrypt") === "on") {
    notice = "E2EE media attachments are not implemented yet.";
    render();
    return;
  }
  if (
    draftAttachments.length > 0 &&
    (visibility === "Followers" || visibility === "Direct") &&
    !draftAttachments.every(isPrivateAttachment)
  ) {
    notice = "Private and direct posts need media uploaded while that visibility is selected.";
    render();
    return;
  }
  const created = await invoke<CreatedPost>("create_owner_post", {
    text,
    visibility,
    protocol,
    encrypt: form.get("encrypt") === "on",
    inReplyTo: draftReplyTo || null,
    recipients: [
      ...form.getAll("follower_recipient").map((value) => String(value)),
      ...String(form.get("recipients") || "")
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean)
    ],
    attachments: draftAttachments
  });
  draftAttachments = [];
  draftReplyTo = "";
  notice = `Published ${created.visibility} post.`;
  active = "Posts";
  await load();
}

async function uploadSelectedMedia() {
  const input = document.querySelector<HTMLInputElement>("#media-file");
  const file = input?.files?.item(0);
  if (!file) {
    notice = "Choose a media file first.";
    render();
    return;
  }
  const form = document.querySelector<HTMLFormElement>("#compose-form");
  const data = form ? new FormData(form) : new FormData();
  const visibility = String(data.get("visibility") || "Followers");
  const protocol = String(data.get("protocol") || "ActivityPub");
  const encrypt = data.get("encrypt") === "on";
  if (protocol !== "ActivityPub") {
    notice = "Media attachments currently require ActivityPub routing.";
    render();
    return;
  }
  if (encrypt) {
    notice = "E2EE media attachments are not implemented yet.";
    render();
    return;
  }
  const access = visibility === "Followers" || visibility === "Direct" ? "private" : "public";
  const dataBase64 = arrayBufferToBase64(await file.arrayBuffer());
  const uploaded = await invoke<UploadedMedia>("upload_owner_media", {
    filename: file.name,
    mediaType: file.type || null,
    access,
    dataBase64
  });
  draftAttachments = [...draftAttachments, JSON.stringify(uploaded.attachment)];
  notice = `Attached ${file.name} as ${uploaded.access || access} media.`;
  render();
}

async function setFollowerStatus(followerActorId: string, status: string) {
  if (!followerActorId || !status) return;
  await invoke("set_follower_status", {
    followerActorId,
    status
  });
  notice = `${actorLabel(followerActorId)} marked ${status}.`;
  await load();
}

async function followActor(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const target = String(form.get("target") || "").trim();
  if (!target) {
    notice = "Enter an ActivityPub actor URL or handle.";
    render();
    return;
  }
  await followTarget(target);
}

async function followTarget(target: string) {
  if (!target) {
    notice = "Enter an ActivityPub actor URL or handle.";
    render();
    return;
  }
  const result = await invoke<FollowResult>("follow_actor", { target });
  notice = `Follow requested for ${actorLabel(result.following.target_actor_id)}.`;
  discoveredActor =
    discoveredActor?.id === result.following.target_actor_id
      ? { ...discoveredActor, following_status: result.following.status }
      : discoveredActor;
  await load();
}

async function discoverActor(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const target = String(form.get("target") || "").trim();
  if (!target) {
    notice = "Enter an ActivityPub actor URL or handle.";
    render();
    return;
  }
  discoveredActor = await invoke<DiscoveredActor>("discover_actor", { target });
  notice = `Resolved ${discoveredActor.handle || actorLabel(discoveredActor.id)}.`;
  render();
}

async function unfollowActor(target: string) {
  if (!target) return;
  const result = await invoke<FollowResult>("unfollow_actor", { target });
  notice = `Unfollow requested for ${actorLabel(result.following.target_actor_id)}.`;
  await load();
}

async function addSource(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const url = String(form.get("url") || "").trim();
  if (!url) {
    notice = "Enter a source URL.";
    render();
    return;
  }
  const title = String(form.get("title") || "").trim();
  const cadence = Number(form.get("cadence_minutes") || 60);
  const result = await invoke<{ source: SourceSubscription }>("add_owner_source", {
    sourceType: String(form.get("source_type") || "rss"),
    url,
    title: title || null,
    cadenceMinutes: Number.isFinite(cadence) ? cadence : 60
  });
  notice = `Added ${result.source.id}.`;
  await loadLiveSection("Sources");
}

async function refreshSource(id: string | null) {
  const result = await invoke<{ ok: boolean; items: Array<{ id: string; ok: boolean; error?: string | null }> }>(
    "refresh_owner_source",
    { id }
  );
  const failures = result.items.filter((item) => !item.ok);
  notice = failures.length
    ? `Refresh completed with ${failures.length} error${failures.length === 1 ? "" : "s"}.`
    : `Refreshed ${result.items.length} source${result.items.length === 1 ? "" : "s"}.`;
  await loadLiveSection("Sources");
}

async function removeSource(id: string) {
  if (!id) return;
  await invoke("remove_owner_source", { id });
  notice = `Removed ${id}.`;
  await loadLiveSection("Sources");
}

async function blockActor(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const actorId = String(form.get("actor_id") || "").trim();
  if (!actorId) {
    notice = "Enter an actor URL.";
    render();
    return;
  }
  await invoke("block_owner_actor", { actorId, reason: null });
  notice = `Blocked ${shortUrl(actorId)}.`;
  await loadLiveSection("Moderation");
}

async function blockDomain(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const domain = String(form.get("domain") || "").trim();
  if (!domain) {
    notice = "Enter a domain.";
    render();
    return;
  }
  await invoke("block_owner_domain", { domain, reason: null });
  notice = `Blocked ${domain}.`;
  await loadLiveSection("Moderation");
}

async function unblockValue(value: string) {
  if (!value) return;
  await invoke("unblock_owner_value", { value });
  notice = `Unblocked ${value}.`;
  await loadLiveSection("Moderation");
}

async function allowHost(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const host = String(form.get("host") || "").trim();
  if (!host) {
    notice = "Enter a host.";
    render();
    return;
  }
  await invoke("allow_owner_host", { host, note: null });
  notice = `Allowed ${host}.`;
  await loadLiveSection("Moderation");
}

async function disallowHost(host: string) {
  if (!host) return;
  await invoke("disallow_owner_host", { host });
  notice = `Removed ${host} from allowlist.`;
  await loadLiveSection("Moderation");
}

async function ownerInteraction(objectId: string, interaction: string) {
  if (!objectId || !interaction) return;
  const result = await invoke<InteractionResult>("owner_interaction", {
    objectId,
    interaction
  });
  notice = `${result.interaction} queued for ${shortUrl(result.object_id)}.`;
  await load();
  if (active === "Posts" && selectedPostDetail?.id === objectId) {
    selectedPostDetail = await invoke<OwnerPostDetail>("owner_post_detail", { objectId });
    render();
  }
}

async function loadPostDetail(objectId: string) {
  if (!objectId) return;
  selectedPostDetail = await invoke<OwnerPostDetail>("owner_post_detail", { objectId });
  notice = `Loaded ${shortUrl(objectId)}.`;
  render();
}

async function copyLink(url: string) {
  if (!url) return;
  await navigator.clipboard.writeText(url);
  notice = `Copied ${shortUrl(url)}.`;
  render();
}

function openLink(url: string) {
  if (!url) return;
  window.open(url, "_blank", "noopener");
  notice = `Opened ${shortUrl(url)}.`;
  render();
}

async function loadLiveSection(section: string) {
  if (section === "Notifications") {
    notifications = await invoke<OwnerNotification[]>("owner_notifications");
    render();
  } else if (section === "Deliveries") {
    deliveries = await invoke<OwnerDelivery[]>("owner_deliveries");
    render();
  } else if (section === "Sources") {
    const sources = await invoke<OwnerSources>("owner_sources");
    sourceSubscriptions = sources.subscriptions;
    sourceItems = sources.items;
    render();
  } else if (section === "Moderation") {
    moderationState = await invoke<ModerationState>("owner_moderation");
    render();
  }
}

async function markNotificationRead(id: string) {
  if (!id) return;
  await invoke("mark_owner_notification_read", { id });
  notice = `Marked notification ${id} read.`;
  await loadLiveSection("Notifications");
}

function isRead(value: OwnerNotification["read"]) {
  return value === true || value === 1 || value === "1" || value === "true";
}

function isEnabled(value: ModerationAllowlistHost["enabled"]) {
  return value === true || value === 1 || value === "1" || value === "true";
}

function option(value: string, selected: boolean) {
  return `<option value="${escapeAttr(value)}"${selected ? " selected" : ""}>${escapeHtml(value)}</option>`;
}

function sectionSubtitle(section: string) {
  if (section === "Compose") return "Private-by-default publishing with explicit public and E2EE modes";
  if (section === "Sources") return "Private reader items from public standards-based sources";
  if (section === "Diagnostics") return "Instance, federation, delivery, and client health";
  return "Owner workspace for the live dais instance";
}

function navGlyph(section: string) {
  return section.slice(0, 1);
}

function shortHost(value: string) {
  try {
    return new URL(value).host;
  } catch {
    return value;
  }
}

function shortUrl(value: string) {
  try {
    const url = new URL(value);
    return `${url.host}${url.pathname}`;
  } catch {
    return value;
  }
}

function arrayBufferToBase64(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function isPrivateAttachment(value: string) {
  try {
    const attachment = JSON.parse(value);
    return typeof attachment.url === "string" && new URL(attachment.url).pathname.startsWith("/media/_private/");
  } catch {
    return false;
  }
}

function stripTags(value: string) {
  return value.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim();
}

function actorLabel(value: string) {
  try {
    const url = new URL(value);
    const username = decodeURIComponent(url.pathname.split("/").filter(Boolean).pop() || url.hostname);
    return `@${username}@${url.hostname}`;
  } catch {
    return value;
  }
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function escapeHtml(value: string) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function escapeAttr(value: string) {
  return escapeHtml(value);
}

load().catch((error) => {
  const app = document.querySelector("#app");
  if (app) {
    app.innerHTML = `<main class="loading error">
      <strong>Owner app could not load.</strong>
      <span>${escapeHtml(String(error))}</span>
    </main>`;
  }
});
