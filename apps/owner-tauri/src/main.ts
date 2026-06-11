import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type OwnerSnapshot = {
  settings: {
    instance_url: string;
    owner_token_present: boolean;
    default_visibility: "Public" | "Unlisted" | "Followers" | "Direct";
    default_protocol: "ActivityPub" | "AtProto" | "Both";
  };
  active_section: string;
  posts: Array<{
    id: string;
    content: string;
    visibility: string;
    protocol: string;
    encrypted: boolean;
    published_at?: string;
  }>;
  sources: Array<{
    id: string;
    title: string;
    source_type: string;
    canonical_url?: string;
    excerpt?: string;
    read: boolean;
  }>;
  moderation: {
    closed_network: boolean;
    block_count: number;
    allowlist_count: number;
  };
  diagnostics: Array<{ key: string; ok: boolean; detail: string }>;
};

const sections = [
  "Home",
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

async function load() {
  snapshot = await invoke<OwnerSnapshot>("owner_snapshot");
  render();
}

function render() {
  if (!snapshot) return;
  const app = document.querySelector<HTMLDivElement>("#app");
  if (!app) return;
  app.innerHTML = `
    <section class="shell">
      <nav class="rail">${sections
        .map(
          (section) =>
            `<button class="${section === active ? "active" : ""}" data-section="${section}">${section}</button>`
        )
        .join("")}</nav>
      <section class="workspace">
        <header>
          <div>
            <h1>${active}</h1>
            <p>${snapshot.settings.instance_url}</p>
          </div>
          <span class="status ${snapshot.settings.owner_token_present ? "ok" : "warn"}">${
            snapshot.settings.owner_token_present ? "token stored" : "owner token needed"
          }</span>
        </header>
        ${view(active, snapshot)}
      </section>
    </section>`;
  app.querySelectorAll<HTMLButtonElement>("[data-section]").forEach((button) => {
    button.addEventListener("click", () => {
      active = button.dataset.section || "Home";
      render();
    });
  });
  app.querySelector<HTMLFormElement>("#settings-form")?.addEventListener("submit", saveSettings);
}

function view(section: string, data: OwnerSnapshot): string {
  switch (section) {
    case "Home":
    case "Posts":
      return composeView() + list(data.posts.map(postCard));
    case "Sources":
      return list(data.sources.map(sourceCard));
    case "Moderation":
      return `<div class="panel"><h2>Federation Safety</h2><p>closed network: ${data.moderation.closed_network}</p><p>blocks: ${data.moderation.block_count}</p><p>allowlist hosts: ${data.moderation.allowlist_count}</p></div>`;
    case "Settings":
      return settingsView(data);
    case "Diagnostics":
      return list(data.diagnostics.map((row) => `<article class="panel"><h2>${row.key}</h2><p>${row.ok ? "ok" : "needs attention"}</p><p>${row.detail}</p></article>`));
    default:
      return `<div class="panel"><h2>${section}</h2><p>Owner API wiring for this screen is tracked in v0.22 follow-up gaps.</p></div>`;
  }
}

function composeView() {
  return `<form class="compose panel">
    <textarea placeholder="Write privately by default"></textarea>
    <div class="controls">
      <button type="button">Private</button>
      <button type="button">Public</button>
      <button type="button">E2EE</button>
    </div>
  </form>`;
}

function settingsView(data: OwnerSnapshot) {
  return `<form id="settings-form" class="panel settings">
    <label>Instance URL<input name="instance" value="${data.settings.instance_url}" /></label>
    <label>Owner token<input name="token" type="password" placeholder="${data.settings.owner_token_present ? "stored" : "required"}" /></label>
    <button>Save</button>
  </form>`;
}

function postCard(post: OwnerSnapshot["posts"][number]) {
  return `<article class="panel"><h2>${post.id}</h2><p>${post.content}</p><p>${post.visibility} / ${post.protocol}${post.encrypted ? " / e2ee" : ""}</p></article>`;
}

function sourceCard(source: OwnerSnapshot["sources"][number]) {
  return `<article class="panel"><h2>${source.title}</h2><p>${source.source_type}${source.read ? " / read" : ""}</p><p>${source.excerpt || ""}</p><a href="${source.canonical_url || "#"}">${source.canonical_url || ""}</a></article>`;
}

function list(items: string[]) {
  return `<section class="list">${items.length ? items.join("") : `<div class="panel"><p>No items yet.</p></div>`}</section>`;
}

async function saveSettings(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.target as HTMLFormElement);
  await invoke("save_owner_settings", {
    instanceUrl: String(form.get("instance") || ""),
    ownerToken: String(form.get("token") || "")
  });
  await load();
}

load().catch((error) => {
  document.querySelector("#app")!.innerHTML = `<pre>${String(error)}</pre>`;
});
