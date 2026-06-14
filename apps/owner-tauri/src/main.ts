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
  }>;
  posts: Array<{
    id: string;
    title?: string | null;
    content: string;
    visibility: Visibility | string;
    protocol: ProtocolRoute | string;
    encrypted: boolean;
    attachments?: Array<{ url?: string; mediaType?: string; name?: string }>;
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
  moderation: {
    closed_network: boolean;
    block_count: number;
    allowlist_count: number;
  };
  diagnostics: Array<{ key: string; ok: boolean; detail: string }>;
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

const sections = [
  "Home",
  "Following",
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

async function load() {
  render();
  snapshot = await invoke<OwnerSnapshot>("owner_snapshot");
  active = active || snapshot.active_section || "Home";
  render();
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
    case "Compose":
      return composeView(data);
    case "Posts":
      return list(data.posts.map(postCard), "No posts returned by the owner API yet.");
    case "Sources":
      return list(data.sources.map(sourceCard), "No source items are available yet.");
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
    case "Deliveries":
      return pendingLiveView(section);
    default:
      return pendingLiveView(section);
  }
}

function dashboardView(data: OwnerSnapshot) {
  return `
    <section class="metrics">
      <article><span>Posts</span><strong>${data.posts.length}</strong></article>
      <article><span>Followers</span><strong>${data.followers.filter((row) => row.status === "approved").length}</strong></article>
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
  return `<section class="split">
    <article class="panel"><h2>Federation safety</h2>
      <p>Closed network: ${data.moderation.closed_network ? "enabled" : "disabled"}</p>
      <p>Blocked actors/domains: ${data.moderation.block_count}</p>
      <p>Allowed hosts: ${data.moderation.allowlist_count}</p>
    </article>
    <article class="panel"><h2>Policy</h2>
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
      ${post.published_at ? `<time>${escapeHtml(formatTime(post.published_at))}</time>` : ""}
    </footer>
  </article>`;
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
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.object_id)}">Reply</button>
      <button type="button" data-timeline-action="like" data-object="${escapeAttr(post.object_id)}">Like</button>
      <button type="button" data-timeline-action="boost" data-object="${escapeAttr(post.object_id)}">Boost</button>
    </footer>
  </article>`;
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
  const result = await invoke<FollowResult>("follow_actor", { target });
  notice = `Follow requested for ${actorLabel(result.following.target_actor_id)}.`;
  await load();
}

async function unfollowActor(target: string) {
  if (!target) return;
  const result = await invoke<FollowResult>("unfollow_actor", { target });
  notice = `Unfollow requested for ${actorLabel(result.following.target_actor_id)}.`;
  await load();
}

async function ownerInteraction(objectId: string, interaction: string) {
  if (!objectId || !interaction) return;
  const result = await invoke<InteractionResult>("owner_interaction", {
    objectId,
    interaction
  });
  notice = `${result.interaction} queued for ${shortUrl(result.object_id)}.`;
  await load();
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
