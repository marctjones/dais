import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Visibility = "Public" | "Unlisted" | "Followers" | "Direct";
type ProtocolRoute = "ActivityPub" | "AtProto" | "Both";
type SensitiveCategory = "medical" | "adult" | "political" | "family-only" | "work-sensitive";

type OwnerAudienceList = {
  id: string;
  name: string;
  description?: string | null;
  allowed_categories: SensitiveCategory[];
  member_actor_ids: string[];
  member_count: number;
  created_at?: string | null;
  updated_at?: string | null;
};

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
  audience_lists: OwnerAudienceList[];
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
  require_authorized_fetch?: boolean;
  manually_approves_followers?: boolean;
  reply_policy?: string;
  ai_enabled?: boolean;
  ai_model?: string | null;
  ai_daily_budget?: number;
  reply_queue_count?: number;
  flagged_reply_count?: number;
  hidden_reply_count?: number;
  rejected_reply_count?: number;
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

type ModerationReply = {
  id: string;
  post_id: string;
  actor_id: string;
  actor_username?: string | null;
  actor_display_name?: string | null;
  actor_avatar_url?: string | null;
  content: string;
  published_at?: string | null;
  created_at?: string | null;
  moderation_status?: string | null;
  moderation_score?: number | null;
  moderation_flags?: string[];
  moderation_checked_at?: string | null;
  ai_moderation?: {
    model?: string | null;
    unsafe_detected?: boolean;
    categories?: string[];
    summary?: string | null;
  } | null;
  hidden: boolean | number | string | null;
};

type CreatedPost = {
  id: string;
  visibility: string;
  protocol: string;
  in_reply_to?: string | null;
  published_at: string;
};

type ComposeDraftState = {
  text: string;
  visibility: Visibility;
  protocol: ProtocolRoute;
  encrypt: boolean;
  audienceListId: string;
  recipients: string;
  selectedRecipients: string[];
};

type FollowResult = {
  ok: boolean;
  following: OwnerSnapshot["following"][number];
  delivery_ids: string[];
};

type DiscoveredActor = {
  id: string;
  actor_type?: string | null;
  inbox: string;
  shared_inbox?: string | null;
  preferred_username?: string | null;
  name?: string | null;
  summary?: string | null;
  url?: string | null;
  icon_url?: string | null;
  handle?: string | null;
  following_status?: string | null;
  target_public_post?: DiscoveredPost | null;
  recent_public_posts?: DiscoveredPost[];
};

type DiscoveredPost = {
  id: string;
  type: string;
  actor_id?: string | null;
  url?: string | null;
  name?: string | null;
  summary?: string | null;
  content: string;
  published?: string | null;
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
  actor_avatar_url?: string | null;
  content?: string | null;
  content_html?: string | null;
  published_at?: string | null;
  created_at?: string | null;
};

type PostInteraction = {
  id: string;
  actor_id: string;
  actor_username?: string | null;
  actor_display_name?: string | null;
  actor_avatar_url?: string | null;
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

type OwnerDirectMessage = {
  id: string;
  conversation_id: string;
  sender_id: string;
  content: string;
  published_at: string;
  created_at?: string | null;
};

type OwnerSearchResult = {
  posts: Array<{
    id: string;
    actor_id?: string | null;
    content: string;
    content_html?: string | null;
    object_type?: string | null;
    name?: string | null;
    summary?: string | null;
    visibility?: string | null;
    protocol?: string | null;
    published_at?: string | null;
    in_reply_to?: string | null;
    atproto_uri?: string | null;
    encrypted_message?: string | null;
    media_attachments?: string | null;
  }>;
  users: Array<{
    actor_id: string;
    relation: string;
    status: string;
    created_at?: string | null;
  }>;
  sources: SourceSubscription[];
  source_items: Array<{
    id: string;
    source_id: string;
    source_type: string;
    title: string;
    canonical_url?: string | null;
    excerpt?: string | null;
    published_at?: string | null;
    read: boolean | number | string | null;
    rights_policy_json: string;
    created_at?: string | null;
  }>;
  public_posts: Array<{
    provider: string;
    network: string;
    id: string;
    url: string;
    content: string;
    actor_id?: string | null;
    actor_handle?: string | null;
    actor_display_name?: string | null;
    content_html?: string | null;
    summary?: string | null;
    object_type?: string | null;
    published_at?: string | null;
    cid?: string | null;
    reply_count?: number | null;
    repost_count?: number | null;
    like_count?: number | null;
  }>;
  public_actors: Array<{
    provider: string;
    network: string;
    id: string;
    handle?: string | null;
    display_name?: string | null;
    summary?: string | null;
    url?: string | null;
    avatar_url?: string | null;
  }>;
  provider_errors: Array<{
    provider: string;
    network: string;
    error: string;
  }>;
};

type OwnerStats = {
  followers_total: number;
  followers_approved: number;
  followers_pending: number;
  followers_rejected: number;
  following_total: number;
  posts_total: number;
  activities_total: number;
  deliveries_total: number;
  deliveries_failed: number;
  deliveries_queued: number;
  deliveries_retry: number;
  deliveries_delivered: number;
  dual_protocol_posts: number;
  public_posts: number;
  private_posts: number;
  direct_posts: number;
  encrypted_posts: number;
  media_posts: number;
  notifications_unread: number;
  blocks_total: number;
  allowlist_hosts: number;
  closed_network: boolean;
};

const sections = [
  "Home",
  "Following",
  "Friends",
  "Audience",
  "Discovery",
  "Compose",
  "Posts",
  "Search",
  "Sources",
  "DMs",
  "Notifications",
  "Followers",
  "Profile",
  "Moderation",
  "Deliveries",
  "Stats",
  "Settings",
  "Diagnostics"
];

const smokeMode = new URLSearchParams(window.location.search).get("smoke") === "1";
const smokePostId = "https://social.dais.social/users/social/posts/smoke-post";

let snapshot: OwnerSnapshot | null = null;
let active = smokeSection() || "Home";
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
let moderationReplies: ModerationReply[] = [];
let directMessages: OwnerDirectMessage[] = [];
let searchQuery = "";
let searchScope = "local";
let searchResults: OwnerSearchResult = emptySearchResults();
let ownerStats: OwnerStats | null = null;
let showTimelineReplies = false;
let showSourceItems = true;
let composeState: ComposeDraftState | null = null;
const supportedSensitiveCategories: SensitiveCategory[] = [
  "medical",
  "adult",
  "political",
  "family-only",
  "work-sensitive"
];

function emptySearchResults(): OwnerSearchResult {
  return {
    posts: [],
    users: [],
    sources: [],
    source_items: [],
    public_posts: [],
    public_actors: [],
    provider_errors: []
  };
}

async function ownerInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!smokeMode) {
    return invoke<T>(command, args);
  }
  return smokeInvoke<T>(command, args);
}

async function smokeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const data = smokeSnapshot();
  if (command === "owner_snapshot") return data as T;
  if (command === "owner_post_detail") return smokePostDetail(String(args?.objectId || smokePostId)) as T;
  if (command === "owner_audience_lists") return smokeSnapshot().audience_lists as T;
  if (command === "upsert_owner_audience_list") {
    const id = String(args?.id || "audience-smoke");
    const name = String(args?.name || "Close friends");
    return {
      id,
      name,
      description: String(args?.description || "Owner-only direct audience list"),
      allowed_categories: (Array.isArray(args?.allowedCategories) ? args?.allowedCategories : args?.allowed_categories || []) as SensitiveCategory[],
      member_actor_ids: (Array.isArray(args?.memberActorIds) ? args?.memberActorIds : args?.member_actor_ids || ["https://mastodon.example/users/alice"]) as string[],
      member_count: Array.isArray(args?.memberActorIds) ? args.memberActorIds.length : 1,
      created_at: "2026-06-16T14:00:00Z",
      updated_at: "2026-06-16T14:00:00Z"
    } as T;
  }
  if (command === "delete_owner_audience_list") return undefined as T;
  if (command === "owner_sources") {
    return {
      subscriptions: [smokeSourceSubscription()],
      items: data.sources
    } as T;
  }
  if (command === "owner_stats") {
    return {
      followers_total: data.followers.length,
      followers_approved: data.followers.filter((row) => row.status === "approved").length,
      followers_pending: data.followers.filter((row) => row.status === "pending").length,
      followers_rejected: data.followers.filter((row) => row.status === "rejected").length,
      following_total: data.following.length,
      posts_total: data.posts.length,
      activities_total: 4,
      deliveries_total: 3,
      deliveries_failed: 0,
      deliveries_queued: 1,
      deliveries_retry: 0,
      deliveries_delivered: 2,
      dual_protocol_posts: 0,
      public_posts: 1,
      private_posts: 1,
      direct_posts: 0,
      encrypted_posts: 0,
      media_posts: 1,
      notifications_unread: 1,
      blocks_total: 0,
      allowlist_hosts: 1,
      closed_network: false
    } as T;
  }
  if (command === "owner_notifications") return [] as T;
  if (command === "owner_deliveries") return [] as T;
  if (command === "owner_direct_messages") return [] as T;
  if (command === "owner_diagnostics") return data.diagnostics as T;
  if (command === "owner_moderation") return data.moderation as T;
  if (command === "owner_moderation_replies") {
    return [
      {
        id: `${smokePostId}#pending-reply`,
        post_id: smokePostId,
        actor_id: "https://mastodon.example/users/alice",
        actor_display_name: "Alice Example",
        content: "This is a medical update reply that should stay in review.",
        published_at: "2026-06-16T14:06:00Z",
        moderation_status: "pending",
        moderation_score: 0.72,
        moderation_flags: ["medical", "ai:medical"],
        ai_moderation: {
          model: "@cf/meta/llama-guard-3-8b",
          unsafe_detected: true,
          categories: ["medical"],
          summary: "Likely private medical content."
        },
        moderation_checked_at: "2026-06-16T14:06:01Z",
        hidden: true
      }
    ] as T;
  }
  if (command === "set_owner_reply_moderation_status") {
    return {
      id: String(args?.replyId || args?.reply_id || `${smokePostId}#pending-reply`),
      post_id: smokePostId,
      actor_id: "https://mastodon.example/users/alice",
      actor_display_name: "Alice Example",
      content: "This is a medical update reply that should stay in review.",
      published_at: "2026-06-16T14:06:00Z",
      moderation_status: String(args?.status || "approved"),
      moderation_score: 0.72,
      moderation_flags: ["medical", "ai:medical"],
      ai_moderation: {
        model: "@cf/meta/llama-guard-3-8b",
        unsafe_detected: true,
        categories: ["medical"],
        summary: "Likely private medical content."
      },
      moderation_checked_at: "2026-06-16T14:06:01Z",
      hidden: String(args?.status || "approved") !== "approved"
    } as T;
  }
  if (command === "save_owner_moderation_settings") {
    return {
      ...data.moderation,
      reply_policy: String(args?.replyPolicy || "review"),
      ai_enabled: Boolean(args?.aiEnabled),
      ai_model: String(args?.aiModel || "@cf/meta/llama-guard-3-8b"),
      ai_daily_budget: Number(args?.aiDailyBudget || 50)
    } as T;
  }
  if (command === "owner_search") {
    return {
      posts: data.posts.map((post) => ({
        id: post.id,
        content: post.content,
        visibility: String(post.visibility),
        protocol: String(post.protocol),
        published_at: post.published_at || null
      })),
      users: data.following.map((row) => ({
        actor_id: row.target_actor_id,
        relation: "following",
        status: row.status,
        created_at: row.created_at || null
      })),
      sources: [smokeSourceSubscription()],
      source_items: data.sources.map((item) => ({
        id: item.id,
        source_id: "source-smoke",
        source_type: item.source_type,
        title: item.title,
        canonical_url: item.canonical_url,
        excerpt: item.excerpt,
        published_at: null,
        read: item.read,
        rights_policy_json: "{}",
        created_at: null
      })),
      public_posts: [
        {
          provider: "bluesky",
          network: "atproto",
          id: "at://did:plc:smoke/app.bsky.feed.post/3smoke",
          url: "https://bsky.app/profile/smoke.example/post/3smoke",
          content: "Public smoke result",
          actor_handle: "smoke.example",
          published_at: "2026-06-17T12:00:00Z"
        }
      ],
      public_actors: [
        {
          provider: "mastodon.social",
          network: "activitypub",
          id: "https://mastodon.social/@smoke",
          handle: "smoke@mastodon.social",
          display_name: "Smoke"
        }
      ],
      provider_errors: []
    } as T;
  }
  if (command === "owner_interaction") {
    return {
      ok: true,
      activity_id: `${smokePostId}#${String(args?.interaction || "like")}`,
      interaction: String(args?.interaction || "like"),
      object_id: String(args?.objectId || smokePostId),
      delivery_ids: ["delivery-smoke"]
    } as T;
  }
  return {} as T;
}

function smokeSection() {
  if (!smokeMode) return "";
  const value = new URLSearchParams(window.location.search).get("section") || "";
  return sections.includes(value) ? value : "Home";
}

function smokeSnapshot(): OwnerSnapshot {
  return {
    settings: {
      instance_url: "https://social.dais.social",
      owner_token_present: true,
      default_visibility: "Followers",
      default_protocol: "ActivityPub"
    },
    profile: {
      id: "actor-social",
      username: "social",
      actor_type: "Person",
      display_name: "Dais Smoke Account",
      summary: "Private-by-default social server smoke profile.",
      public_handle: "@social@dais.social",
      actor_url: "https://social.dais.social/users/social"
    },
    active_section: "Home",
    home_timeline: [
      {
        id: "timeline-smoke",
        object_id: smokePostId,
        actor_id: "https://mastodon.example/users/alice",
        actor_username: "alice",
        actor_display_name: "Alice Example",
        content: "A followed public post with reply, like, and boost actions.",
        visibility: "public",
        published_at: "2026-06-16T14:00:00Z",
        protocol: "activitypub",
        reply_count: 1,
        like_count: 2,
        boost_count: 1
      }
    ],
    posts: [
      {
        id: smokePostId,
        title: "Smoke public post",
        content: "Dais Desk smoke post detail content.",
        visibility: "Public",
        protocol: "ActivityPub",
        encrypted: false,
        attachments: [{ url: "https://social.dais.social/media/smoke.png", mediaType: "image/png", name: "smoke.png" }],
        reply_count: 1,
        like_count: 2,
        boost_count: 1,
        published_at: "2026-06-16T14:00:00Z"
      }
    ],
    followers: [
      {
        id: "follower-smoke",
        actor_id: "https://social.dais.social/users/social",
        follower_actor_id: "https://mastodon.example/users/alice",
        follower_inbox: "https://mastodon.example/inbox",
        status: "approved"
      }
    ],
    friends: [
      {
        friend_actor_id: "https://mastodon.example/users/alice",
        friend_inbox: "https://mastodon.example/inbox",
        accepted_at: "2026-06-16T14:00:00Z"
      }
    ],
    following: [
      {
        id: "following-smoke",
        actor_id: "https://social.dais.social/users/social",
        target_actor_id: "https://mastodon.example/users/alice",
        target_inbox: "https://mastodon.example/inbox",
        status: "accepted",
        accepted_at: "2026-06-16T14:00:00Z"
      }
    ],
    audience_lists: [
      {
        id: "audience-smoke",
        name: "Close friends",
        description: "Small direct audience for sensitive updates.",
        allowed_categories: ["medical", "political"],
        member_actor_ids: ["https://mastodon.example/users/alice"],
        member_count: 1,
        created_at: "2026-06-16T14:00:00Z",
        updated_at: "2026-06-16T14:00:00Z"
      }
    ],
    sources: [
      {
        id: "source-item-smoke",
        title: "Smoke source item",
        source_type: "rss",
        canonical_url: "https://example.com/source-item",
        excerpt: "A private reader item rendered from a public source.",
        read: false
      }
    ],
    moderation: {
      closed_network: false,
      block_count: 0,
      allowlist_count: 1,
      require_authorized_fetch: true,
      manually_approves_followers: true,
      reply_policy: "review",
      ai_enabled: false,
      ai_model: "@cf/meta/llama-guard-3-8b",
      ai_daily_budget: 0,
      reply_queue_count: 1,
      flagged_reply_count: 1,
      hidden_reply_count: 1,
      rejected_reply_count: 0,
      blocks: [],
      allowlist: [{ host: "mastodon.example", enabled: true }]
    },
    diagnostics: [{ key: "owner-api", ok: true, detail: "Smoke fixture owner API" }]
  };
}

function smokePostDetail(objectId: string): OwnerPostDetail {
  return {
    ...smokeSnapshot().posts[0],
    id: objectId || smokePostId,
    in_reply_to: null,
    replies: [
      {
        id: `${smokePostId}#reply`,
        actor_id: "https://mastodon.example/users/alice",
        actor_display_name: "Alice Example",
        content: "Smoke reply rendered in post detail.",
        published_at: "2026-06-16T14:05:00Z"
      }
    ],
    likes: [
      {
        id: `${smokePostId}#like`,
        actor_id: "https://mastodon.example/users/alice",
        actor_display_name: "Alice Example",
        created_at: "2026-06-16T14:06:00Z"
      }
    ],
    boosts: [
      {
        id: `${smokePostId}#boost`,
        actor_id: "https://mastodon.example/users/alice",
        actor_display_name: "Alice Example",
        created_at: "2026-06-16T14:07:00Z"
      }
    ]
  };
}

function smokeSourceSubscription(): SourceSubscription {
  return {
    id: "source-smoke",
    source_type: "rss",
    url: "https://example.com/feed.xml",
    title: "Smoke source",
    status: "active",
    refresh_cadence_minutes: 60,
    error_count: 0,
    policy_json: "{}"
  };
}

async function load() {
  render();
  snapshot = await ownerInvoke<OwnerSnapshot>("owner_snapshot");
  composeState ||= defaultComposeState(snapshot);
  active = active || snapshot.active_section || "Home";
  if (smokeMode && active === "Posts") {
    selectedPostDetail = await ownerInvoke<OwnerPostDetail>("owner_post_detail", { objectId: smokePostId });
  }
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
  app.querySelector<HTMLFormElement>("#compose-form")?.addEventListener("input", updateComposeStateFromForm);
  app.querySelector<HTMLFormElement>("#compose-form")?.addEventListener("change", updateComposeStateFromForm);
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
  app.querySelector<HTMLFormElement>("#search-form")?.addEventListener("submit", runSearch);
  app.querySelector<HTMLFormElement>("#block-actor-form")?.addEventListener("submit", blockActor);
  app.querySelector<HTMLFormElement>("#block-domain-form")?.addEventListener("submit", blockDomain);
  app.querySelector<HTMLFormElement>("#allow-host-form")?.addEventListener("submit", allowHost);
  app.querySelector<HTMLFormElement>("#audience-list-form")?.addEventListener("submit", saveAudienceList);
  app.querySelector<HTMLFormElement>("#moderation-settings-form")?.addEventListener("submit", saveModerationSettings);
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
  app.querySelectorAll<HTMLButtonElement>("[data-reply-status]").forEach((button) => {
    button.addEventListener("click", () => {
      void setReplyModerationStatus(button.dataset.replyId || "", button.dataset.replyStatus || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-audience-edit]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.audienceEdit || "";
      active = "Audience";
      notice = `Editing audience list ${id}.`;
      render();
      const form = document.querySelector<HTMLFormElement>("#audience-list-form");
      const list = snapshot?.audience_lists.find((row) => row.id === id);
      if (!form || !list) return;
      form.elements.namedItem("id") && ((form.elements.namedItem("id") as HTMLInputElement).value = list.id);
      form.elements.namedItem("name") && ((form.elements.namedItem("name") as HTMLInputElement).value = list.name);
      form.elements.namedItem("description") &&
        ((form.elements.namedItem("description") as HTMLTextAreaElement).value = list.description || "");
      form.querySelectorAll<HTMLInputElement>('input[name="allowed_categories"]').forEach((input) => {
        input.checked = list.allowed_categories.includes(input.value as SensitiveCategory);
      });
      form.querySelectorAll<HTMLInputElement>('input[name="member_actor_ids"]').forEach((input) => {
        input.checked = list.member_actor_ids.includes(input.value);
      });
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-audience-delete]").forEach((button) => {
    button.addEventListener("click", () => {
      void deleteAudienceList(button.dataset.audienceDelete || "");
    });
  });
  app.querySelectorAll<HTMLInputElement>("[data-feed-toggle]").forEach((input) => {
    input.addEventListener("change", () => {
      if (input.dataset.feedToggle === "replies") showTimelineReplies = input.checked;
      if (input.dataset.feedToggle === "sources") showSourceItems = input.checked;
      render();
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
    case "Friends":
      return friendsView(data);
    case "Audience":
      return audienceListsView(data);
    case "Discovery":
      return discoveryView();
    case "Compose":
      return composeView(data);
    case "Posts":
      return postsView(data);
    case "Search":
      return searchView();
    case "Sources":
      return sourcesView(data);
    case "DMs":
      return directMessagesView();
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
    case "Stats":
      return statsView(data);
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

function directMessagesView() {
  return `<section>
    ${list(directMessages.map(directMessageCard), "No direct messages returned by the owner API.")}
  </section>`;
}

function searchView() {
  return `<section class="split">
    <article class="panel">
      <h2>Search</h2>
      <form id="search-form" class="inline-form">
        <input name="query" value="${escapeAttr(searchQuery)}" placeholder="Search posts, actors, follows, and sources" />
        <select name="scope">
          <option value="local"${searchScope === "local" ? " selected" : ""}>Local</option>
          <option value="public"${searchScope === "public" ? " selected" : ""}>Public</option>
          <option value="all"${searchScope === "all" ? " selected" : ""}>All</option>
        </select>
        <button type="submit">Search</button>
      </form>
      <h2 class="section-label">Posts</h2>
      ${list(searchResults.posts.map(searchPostCard), "No matching posts.")}
      <h2 class="section-label">Public posts</h2>
      ${list((searchResults.public_posts || []).map(searchPublicPostCard), "No public post results.")}
    </article>
    <article class="panel">
      <h2>Actors</h2>
      ${list(searchResults.users.map(searchUserCard), "No matching actors.")}
      <h2 class="section-label">Public actors</h2>
      ${list((searchResults.public_actors || []).map(searchPublicActorCard), "No public actor results.")}
    </article>
    <article class="panel">
      <h2>Sources</h2>
      ${list((searchResults.sources || []).map(sourceSubscriptionCard), "No matching source subscriptions.")}
      <h2 class="section-label">Source items</h2>
      ${list((searchResults.source_items || []).map(searchSourceItemCard), "No matching source items.")}
      <h2 class="section-label">Providers</h2>
      ${list((searchResults.provider_errors || []).map(searchProviderErrorCard), "Providers returned normally.")}
    </article>
  </section>`;
}

function statsView(data: OwnerSnapshot) {
  const stats = ownerStats;
  if (!stats) {
    return `<section class="metrics">
      <article><span>Posts</span><strong>${data.posts.length}</strong></article>
      <article><span>Followers</span><strong>${data.followers.length}</strong></article>
      <article><span>Following</span><strong>${data.following.length}</strong></article>
      <article><span>Sources</span><strong>${data.sources.length}</strong></article>
    </section>`;
  }
  return `<section class="metrics stats-grid">
    ${metric("Followers", stats.followers_total, `${stats.followers_approved} approved, ${stats.followers_pending} pending`)}
    ${metric("Following", stats.following_total, "")}
    ${metric("Posts", stats.posts_total, `${stats.public_posts} public, ${stats.private_posts} private, ${stats.direct_posts} direct`)}
    ${metric("Media", stats.media_posts, `${stats.encrypted_posts} encrypted, ${stats.dual_protocol_posts} dual-protocol`)}
    ${metric("Deliveries", stats.deliveries_total, `${stats.deliveries_queued} queued, ${stats.deliveries_failed} failed`)}
    ${metric("Notifications", stats.notifications_unread, "unread")}
    ${metric("Moderation", stats.blocks_total, `${stats.allowlist_hosts} allowlist hosts`)}
    ${metric("Network", stats.closed_network ? "closed" : "open", "federation mode")}
  </section>`;
}

function sourcesView(data: OwnerSnapshot) {
  const items = showSourceItems ? (sourceItems.length ? sourceItems : data.sources) : [];
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
      ${sourceControls()}
      ${list(items.map(sourceCard), "No source items are available yet.")}
    </article>
  </section>`;
}

function dashboardView(data: OwnerSnapshot) {
  const timeline = feedTimeline(data);
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
        ${feedControls()}
        ${list(timeline.slice(0, 6).map(timelineCard), "No followed posts yet. Follow people or sources to build this feed.")}
      </article>
    </section>
    <section class="split">
      <div>${list(data.posts.slice(0, 6).map(postCard), "No recent posts.")}</div>
      <div>${list(data.diagnostics.map(diagnosticCard), "No diagnostics.")}</div>
    </section>`;
}

function followingView(data: OwnerSnapshot) {
  const timeline = feedTimeline(data);
  return `<section class="split">
    <article class="panel">
      <h2>Following feed</h2>
      ${feedControls()}
      ${list(timeline.map(timelineCard), "No followed posts yet. Follow an ActivityPub actor to build this feed.")}
    </article>
    <article class="panel">
      <h2>Follow actor</h2>
      <p class="privacy-note">Following is operator-only by default. Treat this list as sensitive; it can reveal medical, adult, political, or private interests.</p>
      <form id="follow-form" class="inline-form">
        <input name="target" placeholder="@user@example.social or https://..." />
        <button type="submit">Follow</button>
      </form>
      <h2 class="section-label">Following</h2>
      ${list(data.following.map(followingCard), "No followed actors yet.")}
    </article>
  </section>`;
}

function friendsView(data: OwnerSnapshot) {
  const timeline = feedTimeline(data);
  return `<section class="split">
    <article class="panel">
      <h2>Mutual friends</h2>
      <p class="privacy-note">Friends are mutual approved relationships. This view is operator-only and should be treated as sensitive social graph data.</p>
      ${list(data.friends.map(friendCard), "No mutual friends yet. A friend appears after they are an approved follower and you follow them back.")}
    </article>
    <article class="panel">
      <h2>Friend feed</h2>
      ${feedControls()}
      ${list(timeline.map(timelineCard), "No friend posts yet. Follow mutual friends to build this feed.")}
    </article>
  </section>`;
}

function audienceListsView(data: OwnerSnapshot) {
  const approvedFollowers = data.followers.filter((row) => row.status === "approved");
  return `<section class="split">
    <article class="panel">
      <div class="section-heading">
        <h2>Audience lists</h2>
        <span class="pill ${data.audience_lists.length ? "ok" : "warn"}">${data.audience_lists.length} list${data.audience_lists.length === 1 ? "" : "s"}</span>
      </div>
      <p class="privacy-note">Audience lists are owner-only direct-routing shortcuts. They should be used for small, intentional sharing sets rather than broad follower distribution.</p>
      ${list(data.audience_lists.map(audienceListCard), "No audience lists yet. Create one for direct sharing with a stable group of approved followers.")}
    </article>
    <article class="panel">
      <h2>Edit list</h2>
      <form id="audience-list-form" class="compact-form">
        <input type="hidden" name="id" value="" />
        <input name="name" placeholder="Close friends" />
        <textarea name="description" placeholder="When this list should be used"></textarea>
        <fieldset class="recipient-picker">
          <legend>Allowed sensitive categories</legend>
          ${supportedSensitiveCategories.map((category) => `<label class="check"><input type="checkbox" name="allowed_categories" value="${escapeAttr(category)}" /> ${escapeHtml(category)}</label>`).join("")}
        </fieldset>
        <fieldset class="recipient-picker">
          <legend>Members</legend>
          ${
            approvedFollowers.length
              ? approvedFollowers.map((row) => `<label class="check"><input type="checkbox" name="member_actor_ids" value="${escapeAttr(row.follower_actor_id)}" /> ${escapeHtml(row.follower_actor_id)}</label>`).join("")
              : `<p>No approved followers are available yet.</p>`
          }
        </fieldset>
        <div class="compose-actions">
          <button type="submit">Save list</button>
        </div>
      </form>
    </article>
  </section>`;
}

function feedTimeline(data: OwnerSnapshot) {
  return showTimelineReplies ? data.home_timeline : data.home_timeline.filter((post) => !post.in_reply_to);
}

function feedControls() {
  return `<div class="feed-controls">
    <label><input type="checkbox" data-feed-toggle="replies" ${showTimelineReplies ? "checked" : ""} /> Show replies</label>
    <span>Chronological, not engagement ranked</span>
  </div>`;
}

function sourceControls() {
  return `<div class="feed-controls">
    <label><input type="checkbox" data-feed-toggle="sources" ${showSourceItems ? "checked" : ""} /> Show source items</label>
    <span>Private reader-only by default</span>
  </div>`;
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
  const state = ensureComposeState(data);
  const approvedFollowers = data.followers.filter((row) => row.status === "approved");
  const availableAudienceLists = data.audience_lists;
  return `<form id="compose-form" class="panel compose">
    <div class="compose-head">
      <h2>New post</h2>
      <span class="pill ok">Private default</span>
    </div>
    <textarea name="text" placeholder="Write to approved followers by default">${escapeHtml(state.text)}</textarea>
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
          ${option("Followers", state.visibility === "Followers")}
          ${option("Public", state.visibility === "Public")}
          ${option("Unlisted", state.visibility === "Unlisted")}
          ${option("Direct", state.visibility === "Direct")}
        </select>
      </label>
      <label>Protocol
        <select name="protocol">
          ${option("ActivityPub", state.protocol === "ActivityPub")}
          ${option("Both", state.protocol === "Both")}
          ${option("AtProto", state.protocol === "AtProto")}
        </select>
      </label>
      <label>Audience list
        <select name="audience_list_id">
          <option value="">None</option>
          ${availableAudienceLists.map((list) => `<option value="${escapeAttr(list.id)}"${state.audienceListId === list.id ? " selected" : ""}>${escapeHtml(list.name)} (${list.member_count})</option>`).join("")}
        </select>
      </label>
    </div>
    <p class="privacy-note">Public is internet-visible. Followers goes to approved followers. Direct is for named recipients only.</p>
    <label>Recipients
      <input name="recipients" value="${escapeAttr(state.recipients)}" placeholder="Direct/E2EE actor URLs, comma separated" />
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
    <section id="compose-preview" class="compose-preview">
      ${composePreviewHtml(data, state)}
    </section>
    <fieldset class="recipient-picker">
      <legend>Approved followers</legend>
      ${
        approvedFollowers.length
          ? approvedFollowers
              .map((row) => recipientOption(row, state.selectedRecipients.includes(row.follower_actor_id)))
              .join("")
          : `<p>No approved followers are available for direct selection.</p>`
      }
    </fieldset>
    <div class="compose-actions">
      <label class="check"><input name="encrypt" type="checkbox"${state.encrypt ? " checked" : ""} /> E2EE</label>
      <button type="submit">Publish</button>
    </div>
  </form>`;
}

function defaultComposeState(data: OwnerSnapshot): ComposeDraftState {
  return {
    text: "",
    visibility: data.settings.default_visibility,
    protocol: data.settings.default_protocol,
    encrypt: false,
    audienceListId: "",
    recipients: "",
    selectedRecipients: [],
  };
}

function ensureComposeState(data: OwnerSnapshot) {
  if (!composeState) {
    composeState = defaultComposeState(data);
  }
  return composeState;
}

function readComposeState(form: HTMLFormElement, data: OwnerSnapshot): ComposeDraftState {
  return {
    text: String(form.get("text") || ""),
    visibility: normalizeVisibility(String(form.get("visibility") || data.settings.default_visibility)),
    protocol: normalizeProtocol(String(form.get("protocol") || data.settings.default_protocol)),
    encrypt: form.get("encrypt") === "on",
    audienceListId: String(form.get("audience_list_id") || ""),
    recipients: String(form.get("recipients") || ""),
    selectedRecipients: form
      .getAll("follower_recipient")
      .map((value) => String(value))
      .filter(Boolean),
  };
}

function syncComposePreview() {
  if (!snapshot || !composeState) return;
  const preview = document.querySelector<HTMLElement>("#compose-preview");
  if (preview) {
    preview.innerHTML = composePreviewHtml(snapshot, composeState);
  }
}

function updateComposeStateFromForm(event: Event) {
  const form = event.currentTarget as HTMLFormElement | null;
  if (!form || !snapshot) return;
  composeState = readComposeState(form, snapshot);
  syncComposePreview();
}

function composePreviewHtml(data: OwnerSnapshot, state: ComposeDraftState) {
  const audienceList = selectedAudienceList(data, state);
  const recipients = resolvedDraftRecipients(data, state);
  const audience = audienceForCompose(
    state.visibility,
    data.followers.filter((row) => row.status === "approved").length,
    recipients.length,
    audienceList
  );
  const sensitiveCategories = sensitiveCategoriesForText(state.text);
  const warnings = composeWarnings(state, sensitiveCategories, audienceList, recipients.length);
  return `<article class="preview-card">
    <div class="section-heading">
      <h3>Audience preview</h3>
      <span class="pill ${state.visibility === "Public" ? "warn" : "ok"}">${escapeHtml(audienceLabel(state.visibility))}</span>
    </div>
    <p>${escapeHtml(audience)}</p>
    <dl class="preview-meta">
      <dt>Protocol</dt><dd>${escapeHtml(state.protocol)}</dd>
      <dt>Audience list</dt><dd>${audienceList ? escapeHtml(`${audienceList.name} (${audienceList.member_count})`) : "none"}</dd>
      <dt>Recipients</dt><dd>${escapeHtml(recipientSummary(recipients))}</dd>
      <dt>Reply target</dt><dd>${draftReplyTo ? escapeHtml(shortUrl(draftReplyTo)) : "none"}</dd>
      <dt>Attachments</dt><dd>${draftAttachments.length ? escapeHtml(String(draftAttachments.length)) : "none"}</dd>
    </dl>
    <div class="preview-tags">
      ${sensitiveCategories.length ? sensitiveCategories.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("") : `<span class="sensitive-chip neutral">No obvious sensitive content</span>`}
    </div>
    ${
      warnings.length
        ? `<div class="warning-list">${warnings.map((warning) => `<p class="privacy-warning">${escapeHtml(warning)}</p>`).join("")}</div>`
        : `<p class="privacy-note">No routing or sensitivity warnings detected for this draft.</p>`
    }
    ${
      recipients.length
        ? `<div class="preview-recipient-list">
            <span class="section-label">Recipients</span>
            ${recipients.map((value) => `<span class="pill">${escapeHtml(shortUrl(value))}</span>`).join("")}
          </div>`
        : ""
    }
  </article>`;
}

function audienceForCompose(
  visibility: Visibility,
  approvedCount: number,
  recipientCount: number,
  audienceList: OwnerAudienceList | null
) {
  if (visibility === "Public") return "Public posts are visible on the open web and public feeds.";
  if (visibility === "Unlisted") return "Unlisted posts are link-visible but stay out of most public listings.";
  if (visibility === "Followers") return `Followers-only posts reach ${approvedCount} approved follower${approvedCount === 1 ? "" : "s"}.`;
  if (audienceList) {
    return `Direct posts reach ${recipientCount} recipient${recipientCount === 1 ? "" : "s"} through ${audienceList.name}.`;
  }
  return recipientCount
    ? `Direct posts reach ${recipientCount} named recipient${recipientCount === 1 ? "" : "s"}.`
    : "Direct posts need at least one named recipient before publish.";
}

function sensitiveCategoriesForText(text: string): SensitiveCategory[] {
  const lower = text.toLowerCase();
  const rules = [
    { label: "medical", keywords: ["medical", "doctor", "clinic", "hospital", "therapy", "medication", "prescription", "surgery", "diagnosis", "health"] },
    { label: "adult", keywords: ["adult", "nsfw", "sexual", "sex", "porn", "erotic", "explicit"] },
    { label: "political", keywords: ["political", "politics", "election", "vote", "campaign", "senate", "congress", "democrat", "republican"] },
    { label: "family-only", keywords: ["family", "kids", "child", "children", "baby", "spouse", "partner", "wedding"] },
    { label: "work-sensitive", keywords: ["work", "company", "employer", "client", "salary", "interview", "manager", "confidential", "internal", "project"] },
  ];
  return rules
    .filter((rule) => rule.keywords.some((keyword) => lower.includes(keyword)))
    .map((rule) => rule.label as SensitiveCategory);
}

function sensitivityBadgesHtml(text: string) {
  const categories = sensitiveCategoriesForText(text);
  return categories.length
    ? `<div class="sensitivity-tags">${categories.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("")}</div>`
    : "";
}

function composeWarnings(
  state: ComposeDraftState,
  sensitiveCategories: SensitiveCategory[],
  audienceList: OwnerAudienceList | null,
  recipientCount: number
) {
  const warnings: string[] = [];
  if (state.protocol !== "ActivityPub") {
    if (state.visibility === "Public") {
      warnings.push("Public Bluesky routing is visible outside the private ActivityPub audience.");
    } else if (state.visibility === "Direct") {
      warnings.push("Direct posts cannot be represented on Bluesky; route ActivityPub only.");
    } else {
      warnings.push("Private ActivityPub visibility is not representable on Bluesky.");
    }
  }
  if (state.visibility === "Direct" && recipientCount === 0) {
    warnings.push("Direct posts need at least one named recipient.");
  }
  if (audienceList && state.visibility !== "Direct") {
    warnings.push("Audience lists currently apply only to direct posts.");
  }
  if (sensitiveCategories.length) {
    const categoryList = sensitiveCategories.join(", ");
    const disallowed =
      audienceList ? sensitiveCategories.filter((label) => !audienceList.allowed_categories.includes(label)) : [];
    if (state.visibility === "Public") {
      warnings.push(`Sensitive content detected (${categoryList}). Public posts are hard to retract.`);
    } else if (state.visibility === "Unlisted") {
      warnings.push(`Sensitive content detected (${categoryList}). Unlisted posts still spread by link.`);
    } else if (state.visibility === "Followers") {
      warnings.push(`Sensitive content detected (${categoryList}). Approved followers can still include people you do not expect.`);
    } else {
      warnings.push(`Sensitive content detected (${categoryList}). Only the named recipients will see this post.`);
    }
    if (disallowed.length) {
      warnings.push(`Selected audience list does not permit ${disallowed.join(", ")} content.`);
    }
  }
  if (draftAttachments.length > 0 && state.protocol !== "ActivityPub") {
    warnings.push("Media attachments currently require ActivityPub routing.");
  }
  if (draftAttachments.length > 0 && state.encrypt) {
    warnings.push("E2EE media attachments are not implemented yet.");
  }
  return warnings;
}

function draftRecipients(state: ComposeDraftState) {
  const explicitRecipients = state.recipients
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  return Array.from(new Set([...state.selectedRecipients, ...explicitRecipients]));
}

function selectedAudienceList(data: OwnerSnapshot, state: ComposeDraftState) {
  return data.audience_lists.find((list) => list.id === state.audienceListId) || null;
}

function resolvedDraftRecipients(data: OwnerSnapshot, state: ComposeDraftState) {
  const explicit = draftRecipients(state);
  const fromList = selectedAudienceList(data, state)?.member_actor_ids || [];
  return Array.from(new Set([...fromList, ...explicit]));
}

function recipientSummary(recipients: string[]) {
  if (!recipients.length) {
    return "No named recipients selected.";
  }
  if (recipients.length === 1) {
    return `1 recipient: ${shortUrl(recipients[0])}`;
  }
  return `${recipients.length} recipients selected.`;
}

function followersView(data: OwnerSnapshot) {
  const pending = data.followers.filter((row) => row.status === "pending");
  const approved = data.followers.filter((row) => row.status === "approved");
  const rejected = data.followers.filter((row) => row.status === "rejected");
  return `<section class="split followers">
    <div>
      <p class="privacy-note">Follower lists are owner-token views. Dais does not advertise them publicly by default.</p>
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
      <p>Reply policy: ${escapeHtml(moderation.reply_policy || "warn")}</p>
      <p>Reply queue: ${moderation.reply_queue_count || 0} pending, ${moderation.flagged_reply_count || 0} flagged</p>
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
      <form id="moderation-settings-form" class="compact-form">
        <label>Reply policy
          <select name="reply_policy">
            ${["off", "warn", "review", "hide", "reject"].map((value) => option(value, (moderation.reply_policy || "warn") === value)).join("")}
          </select>
        </label>
        <label class="check"><input type="checkbox" name="ai_enabled"${moderation.ai_enabled ? " checked" : ""} /> Workers AI live advisory mode</label>
        <label>AI model
          <input name="ai_model" value="${escapeAttr(moderation.ai_model || "@cf/meta/llama-guard-3-8b")}" />
        </label>
        <label>Daily AI budget
          <input name="ai_daily_budget" type="number" min="0" value="${escapeAttr(String(moderation.ai_daily_budget || 0))}" />
        </label>
        <button type="submit">Save policy</button>
      </form>
      <p>Deterministic rules remain authoritative. Workers AI adds advisory flags and summaries when enabled.</p>
      <p>Private posts stay off public outboxes and Bluesky routes.</p>
      <p>Public routing is explicit from compose.</p>
    </article>
    <article class="panel">
      <h2>Reply queue</h2>
      ${list(moderationReplies.map(moderationReplyCard), "No flagged or queued replies.")}
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
      ${sensitivityBadgesHtml(post.content)}
    </div>
    <footer>
      <span title="${escapeAttr(audienceDescription(post.visibility))}">${escapeHtml(audienceLabel(post.visibility))}</span>
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
      <dt>Audience</dt><dd>${escapeHtml(audienceLabel(post.visibility))}</dd>
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
            ${row.content ? sensitivityBadgesHtml(row.content) : ""}
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
      ${sensitivityBadgesHtml(post.content)}
    </div>
    <footer>
      <span title="${escapeAttr(audienceDescription(post.visibility))}">${escapeHtml(audienceLabel(post.visibility))}</span>
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

function audienceLabel(value: unknown) {
  const normalized = String(value || "unknown").toLowerCase();
  if (normalized === "public") return "Public - internet visible";
  if (normalized === "unlisted") return "Unlisted - link visible";
  if (normalized === "followers" || normalized === "private") return "Followers - approved followers";
  if (normalized === "direct") return "Direct - named recipients";
  return `${String(value || "Unknown")} - check audience`;
}

function audienceDescription(value: unknown) {
  const normalized = String(value || "unknown").toLowerCase();
  if (normalized === "public") return "Visible on public web, public ActivityPub and Mastodon surfaces, and enabled public protocol routes.";
  if (normalized === "unlisted") return "Reachable by URL but kept out of public listing surfaces where supported.";
  if (normalized === "followers" || normalized === "private") return "Visible to approved followers; kept out of anonymous public feeds.";
  if (normalized === "direct") return "Intended only for named recipients, not general friends or public feeds.";
  return "Verify post details before assuming who can see this.";
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

function directMessageCard(row: OwnerDirectMessage) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(actorLabel(row.sender_id))}</h2>
      <p>${escapeHtml(row.content)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.conversation_id)}</span>
      <time>${escapeHtml(formatTime(row.published_at))}</time>
    </footer>
  </article>`;
}

function searchPostCard(row: OwnerSearchResult["posts"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.name || shortUrl(row.id))}</h2>
      <p>${escapeHtml(row.content)}</p>
      ${sensitivityBadgesHtml([row.name || "", row.summary || "", row.content || ""].join(" "))}
    </div>
    <footer>
      <span title="${escapeAttr(audienceDescription(row.visibility))}">${escapeHtml(audienceLabel(row.visibility))}</span>
      <span>${escapeHtml(row.protocol || "activitypub")}</span>
      ${row.encrypted_message ? "<span>E2EE</span>" : ""}
      ${row.media_attachments ? "<span>media</span>" : ""}
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      <button type="button" data-post-detail="${escapeAttr(row.id)}">Detail</button>
    </footer>
  </article>`;
}

function searchUserCard(row: OwnerSearchResult["users"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(actorLabel(row.actor_id))}</h2>
      <p>${escapeHtml(row.actor_id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.relation)}</span>
      <span>${escapeHtml(row.status)}</span>
      ${row.created_at ? `<time>${escapeHtml(formatTime(row.created_at))}</time>` : ""}
    </footer>
  </article>`;
}

function searchPublicPostCard(row: OwnerSearchResult["public_posts"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.actor_display_name || row.actor_handle || shortUrl(row.url))}</h2>
      <p>${escapeHtml(row.content || row.summary || "")}</p>
      ${sensitivityBadgesHtml([row.actor_display_name || "", row.actor_handle || "", row.content || "", row.summary || ""].join(" "))}
    </div>
    <footer>
      <span>${escapeHtml(row.network)}</span>
      <span>${escapeHtml(row.provider)}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      <a href="${escapeAttr(row.url)}">${escapeHtml(shortHost(row.url))}</a>
    </footer>
  </article>`;
}

function searchPublicActorCard(row: OwnerSearchResult["public_actors"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.display_name || row.handle || actorLabel(row.id))}</h2>
      <p>${escapeHtml(row.handle || row.id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.network)}</span>
      <span>${escapeHtml(row.provider)}</span>
      ${row.url ? `<a href="${escapeAttr(row.url)}">${escapeHtml(shortHost(row.url))}</a>` : ""}
    </footer>
  </article>`;
}

function searchProviderErrorCard(row: OwnerSearchResult["provider_errors"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.provider)}</h2>
      <p>${escapeHtml(row.error)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.network)}</span>
    </footer>
  </article>`;
}

function searchSourceItemCard(row: OwnerSearchResult["source_items"][number]) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.title)}</h2>
      ${row.excerpt ? `<p>${escapeHtml(row.excerpt)}</p>` : ""}
    </div>
    <footer>
      <span>${escapeHtml(row.source_type)}</span>
      <span>${row.read ? "Read" : "Unread"}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      ${row.canonical_url ? `<a href="${escapeAttr(row.canonical_url)}">${escapeHtml(shortHost(row.canonical_url))}</a>` : ""}
    </footer>
  </article>`;
}

function metric(label: string, value: string | number, detail: string) {
  return `<article>
    <span>${escapeHtml(label)}</span>
    <strong>${escapeHtml(String(value))}</strong>
    ${detail ? `<small>${escapeHtml(detail)}</small>` : ""}
  </article>`;
}

function audienceListCard(row: OwnerAudienceList) {
  return `<article class="panel item follower">
    <div>
      <h2>${escapeHtml(row.name)}</h2>
      ${row.description ? `<p>${escapeHtml(row.description)}</p>` : ""}
      <p>${escapeHtml(`${row.member_count} member${row.member_count === 1 ? "" : "s"}`)}</p>
      <div class="sensitivity-tags">
        ${
          row.allowed_categories.length
            ? row.allowed_categories.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("")
            : `<span class="sensitive-chip neutral">No sensitive categories allowed</span>`
        }
      </div>
    </div>
    <footer>
      <span>${escapeHtml(row.id)}</span>
      <button type="button" data-audience-edit="${escapeAttr(row.id)}">Edit</button>
      <button type="button" data-audience-delete="${escapeAttr(row.id)}">Delete</button>
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

function friendCard(row: OwnerSnapshot["friends"][number]) {
  return `<article class="panel item follower">
    <div>
      <h2>${escapeHtml(actorLabel(row.friend_actor_id))}</h2>
      <p>${escapeHtml(row.friend_actor_id)}</p>
    </div>
    <footer>
      <span>Mutual</span>
      ${row.accepted_at ? `<time>${escapeHtml(formatTime(row.accepted_at))}</time>` : ""}
      ${row.friend_shared_inbox ? `<span>${escapeHtml(shortHost(row.friend_shared_inbox))}</span>` : ""}
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
      ${actor.actor_type ? `<p>${escapeHtml(actor.actor_type)}</p>` : ""}
      ${actor.handle ? `<p>${escapeHtml(actor.handle)}</p>` : ""}
      ${actor.summary ? `<p>${escapeHtml(stripTags(actor.summary))}</p>` : ""}
      ${sensitivityBadgesHtml([title, actor.handle || "", actor.summary || "", actor.id].join(" "))}
      ${actor.target_public_post ? `
        <h2 class="section-label">Requested public post</h2>
        ${discoveredPostCard(actor.target_public_post)}
      ` : ""}
      <h2 class="section-label">Recent public posts</h2>
      ${list((actor.recent_public_posts || []).map(discoveredPostCard), "No recent public posts returned.")}
    </div>
    <footer>
      <span>${escapeHtml(status)}</span>
      <a href="${escapeAttr(actor.url || actor.id)}">${escapeHtml(shortUrl(actor.url || actor.id))}</a>
      <span>${escapeHtml(shortUrl(actor.inbox))}</span>
      <button type="button" data-follow-discovered="1">Follow</button>
    </footer>
  </article>`;
}

function discoveredPostCard(post: DiscoveredPost) {
  const title = post.name || post.content || post.url || post.id;
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(title)}</h2>
      ${post.content && post.content !== title ? `<p>${escapeHtml(post.content)}</p>` : ""}
      ${sensitivityBadgesHtml([post.name || "", post.summary || "", post.content || ""].join(" "))}
    </div>
    <footer>
      <span>${escapeHtml(post.type)}</span>
      ${post.actor_id ? `<span>${escapeHtml(actorLabel(post.actor_id))}</span>` : ""}
      ${post.published ? `<time>${escapeHtml(formatTime(post.published))}</time>` : ""}
      ${post.url ? `<a href="${escapeAttr(post.url)}">${escapeHtml(shortUrl(post.url))}</a>` : ""}
    </footer>
  </article>`;
}

function recipientOption(row: OwnerSnapshot["followers"][number], checked: boolean) {
  const value = row.follower_actor_id;
  return `<label class="recipient-option">
    <input type="checkbox" name="follower_recipient" value="${escapeAttr(value)}"${checked ? " checked" : ""} />
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

function moderationReplyCard(row: ModerationReply) {
  const ai = row.ai_moderation;
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.actor_display_name || row.actor_username || actorLabel(row.actor_id))}</h2>
      <p>${escapeHtml(row.content)}</p>
      ${row.moderation_flags?.length ? `<div class="sensitivity-tags">${row.moderation_flags.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("")}</div>` : ""}
      ${ai?.summary ? `<p class="privacy-note">AI advisory${ai.model ? ` (${escapeHtml(shortModel(ai.model))})` : ""}: ${escapeHtml(ai.summary)}</p>` : ""}
    </div>
    <footer>
      <span>${escapeHtml(row.moderation_status || "approved")}</span>
      ${row.moderation_score != null ? `<span>${escapeHtml(row.moderation_score.toFixed(2))}</span>` : ""}
      <span>${isHiddenValue(row.hidden) ? "hidden" : "visible"}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      <button type="button" data-reply-id="${escapeAttr(row.id)}" data-reply-status="approved">Approve</button>
      <button type="button" data-reply-id="${escapeAttr(row.id)}" data-reply-status="hidden">Hide</button>
      <button type="button" data-reply-id="${escapeAttr(row.id)}" data-reply-status="rejected">Reject</button>
    </footer>
  </article>`;
}

function shortModel(value: string) {
  return value.split("/").at(-1) || value;
}

function list(items: string[], emptyText: string) {
  return `<section class="list">${items.length ? items.join("") : `<article class="panel empty"><p>${escapeHtml(emptyText)}</p></article>`}</section>`;
}

async function saveSettings(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.target as HTMLFormElement);
  await ownerInvoke("save_owner_settings", {
    instanceUrl: String(form.get("instance") || ""),
    ownerToken: String(form.get("token") || "")
  });
  notice = "Settings saved.";
  await load();
}

async function saveProfile(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const updated = await ownerInvoke<OwnerSnapshot["profile"]>("update_owner_profile", {
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
  if (snapshot) {
    composeState = readComposeState(event.target as HTMLFormElement, snapshot);
  }
  const visibility = String(form.get("visibility") || "Followers");
  const protocol = String(form.get("protocol") || "ActivityPub");
  const audienceListId = String(form.get("audience_list_id") || "").trim();
  const audienceList = snapshot && composeState ? selectedAudienceList(snapshot, composeState) : null;
  const disallowedCategories =
    audienceList && composeState
      ? sensitiveCategoriesForText(composeState.text).filter((label) => !audienceList.allowed_categories.includes(label))
      : [];
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
  if (audienceListId && visibility !== "Direct") {
    notice = "Audience lists currently require direct visibility.";
    render();
    return;
  }
  if (disallowedCategories.length) {
    notice = `Selected audience list does not permit ${disallowedCategories.join(", ")} content.`;
    render();
    return;
  }
  const created = await ownerInvoke<CreatedPost>("create_owner_post", {
    text,
    visibility,
    protocol,
    encrypt: form.get("encrypt") === "on",
    inReplyTo: draftReplyTo || null,
    audienceListId: audienceListId || null,
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
  composeState = null;
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
  const uploaded = await ownerInvoke<UploadedMedia>("upload_owner_media", {
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
  await ownerInvoke("set_follower_status", {
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
  const result = await ownerInvoke<FollowResult>("follow_actor", { target });
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
  discoveredActor = await ownerInvoke<DiscoveredActor>("discover_actor", { target });
  notice = `Resolved ${discoveredActor.handle || actorLabel(discoveredActor.id)}.`;
  render();
}

async function unfollowActor(target: string) {
  if (!target) return;
  const result = await ownerInvoke<FollowResult>("unfollow_actor", { target });
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
  const result = await ownerInvoke<{ source: SourceSubscription }>("add_owner_source", {
    sourceType: String(form.get("source_type") || "rss"),
    url,
    title: title || null,
    cadenceMinutes: Number.isFinite(cadence) ? cadence : 60
  });
  notice = `Added ${result.source.id}.`;
  await loadLiveSection("Sources");
}

async function refreshSource(id: string | null) {
  const result = await ownerInvoke<{ ok: boolean; items: Array<{ id: string; ok: boolean; error?: string | null }> }>(
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
  await ownerInvoke("remove_owner_source", { id });
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
  await ownerInvoke("block_owner_actor", { actorId, reason: null });
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
  await ownerInvoke("block_owner_domain", { domain, reason: null });
  notice = `Blocked ${domain}.`;
  await loadLiveSection("Moderation");
}

async function unblockValue(value: string) {
  if (!value) return;
  await ownerInvoke("unblock_owner_value", { value });
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
  await ownerInvoke("allow_owner_host", { host, note: null });
  notice = `Allowed ${host}.`;
  await loadLiveSection("Moderation");
}

async function saveAudienceList(event: SubmitEvent) {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const fields = new FormData(form);
  const name = String(fields.get("name") || "").trim();
  if (!name) {
    notice = "Audience list name is required.";
    render();
    return;
  }
  const saved = await ownerInvoke<OwnerAudienceList>("upsert_owner_audience_list", {
    id: String(fields.get("id") || "").trim() || null,
    name,
    description: String(fields.get("description") || "").trim() || null,
    allowedCategories: fields.getAll("allowed_categories").map((value) => String(value)),
    memberActorIds: fields.getAll("member_actor_ids").map((value) => String(value)),
  });
  form.reset();
  notice = `Saved audience list ${saved.name}.`;
  await load();
  active = "Audience";
  await loadLiveSection("Audience");
}

async function saveModerationSettings(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  moderationState = await ownerInvoke<ModerationState>("save_owner_moderation_settings", {
    replyPolicy: String(form.get("reply_policy") || "warn"),
    aiEnabled: form.get("ai_enabled") === "on",
    aiModel: String(form.get("ai_model") || "").trim() || null,
    aiDailyBudget: Number(form.get("ai_daily_budget") || 0),
  });
  notice = "Saved reply moderation settings.";
  await loadLiveSection("Moderation");
}

async function setReplyModerationStatus(replyId: string, status: string) {
  if (!replyId || !status) return;
  await ownerInvoke<ModerationReply>("set_owner_reply_moderation_status", { replyId, status });
  notice = `Set reply ${shortUrl(replyId)} to ${status}.`;
  await loadLiveSection("Moderation");
  if (active === "Posts" && selectedPostDetail?.id) {
    selectedPostDetail = await ownerInvoke<OwnerPostDetail>("owner_post_detail", { objectId: selectedPostDetail.id });
    render();
  }
}

async function deleteAudienceList(id: string) {
  if (!id) return;
  await ownerInvoke("delete_owner_audience_list", { id });
  notice = `Deleted audience list ${id}.`;
  await load();
  active = "Audience";
  await loadLiveSection("Audience");
}

async function disallowHost(host: string) {
  if (!host) return;
  await ownerInvoke("disallow_owner_host", { host });
  notice = `Removed ${host} from allowlist.`;
  await loadLiveSection("Moderation");
}

async function ownerInteraction(objectId: string, interaction: string) {
  if (!objectId || !interaction) return;
  const result = await ownerInvoke<InteractionResult>("owner_interaction", {
    objectId,
    interaction
  });
  notice = `${result.interaction} queued for ${shortUrl(result.object_id)}.`;
  await load();
  if (active === "Posts" && selectedPostDetail?.id === objectId) {
    selectedPostDetail = await ownerInvoke<OwnerPostDetail>("owner_post_detail", { objectId });
    render();
  }
}

async function loadPostDetail(objectId: string) {
  if (!objectId) return;
  selectedPostDetail = await ownerInvoke<OwnerPostDetail>("owner_post_detail", { objectId });
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
    notifications = await ownerInvoke<OwnerNotification[]>("owner_notifications");
    render();
  } else if (section === "Deliveries") {
    deliveries = await ownerInvoke<OwnerDelivery[]>("owner_deliveries");
    render();
  } else if (section === "DMs") {
    directMessages = await ownerInvoke<OwnerDirectMessage[]>("owner_direct_messages");
    render();
  } else if (section === "Stats") {
    ownerStats = await ownerInvoke<OwnerStats>("owner_stats");
    render();
  } else if (section === "Diagnostics") {
    const diagnostics = await ownerInvoke<OwnerSnapshot["diagnostics"]>("owner_diagnostics");
    snapshot = snapshot ? { ...snapshot, diagnostics } : snapshot;
    render();
  } else if (section === "Sources") {
    const sources = await ownerInvoke<OwnerSources>("owner_sources");
    sourceSubscriptions = sources.subscriptions;
    sourceItems = sources.items;
    render();
  } else if (section === "Moderation") {
    moderationState = await ownerInvoke<ModerationState>("owner_moderation");
    moderationReplies = await ownerInvoke<ModerationReply[]>("owner_moderation_replies");
    render();
  } else if (section === "Audience") {
    const audienceLists = await ownerInvoke<OwnerAudienceList[]>("owner_audience_lists");
    snapshot = snapshot ? { ...snapshot, audience_lists: audienceLists } : snapshot;
    render();
  }
}

async function runSearch(event: Event) {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const query = String(new FormData(form).get("query") || "").trim();
  const scope = String(new FormData(form).get("scope") || "local").trim() || "local";
  searchQuery = query;
  searchScope = scope;
  searchResults = query
    ? normalizeSearchResults(await ownerInvoke<OwnerSearchResult>("owner_search", { query, scope }))
    : emptySearchResults();
  notice = query
    ? `Search returned ${searchResults.posts.length} posts, ${searchResults.users.length} actors, ${(searchResults.sources || []).length} sources, ${(searchResults.source_items || []).length} source items, ${(searchResults.public_posts || []).length} public posts, and ${(searchResults.public_actors || []).length} public actors.`
    : "";
  render();
}

function normalizeSearchResults(results: OwnerSearchResult): OwnerSearchResult {
  return {
    ...emptySearchResults(),
    ...results,
    posts: results.posts || [],
    users: results.users || [],
    sources: results.sources || [],
    source_items: results.source_items || [],
    public_posts: results.public_posts || [],
    public_actors: results.public_actors || [],
    provider_errors: results.provider_errors || []
  };
}

async function markNotificationRead(id: string) {
  if (!id) return;
  await ownerInvoke("mark_owner_notification_read", { id });
  notice = `Marked notification ${id} read.`;
  await loadLiveSection("Notifications");
}

function isRead(value: OwnerNotification["read"]) {
  return value === true || value === 1 || value === "1" || value === "true";
}

function isEnabled(value: ModerationAllowlistHost["enabled"]) {
  return value === true || value === 1 || value === "1" || value === "true";
}

function isHiddenValue(value: ModerationReply["hidden"]) {
  return value === true || value === 1 || value === "1" || value === "true";
}

function option(value: string, selected: boolean) {
  return `<option value="${escapeAttr(value)}"${selected ? " selected" : ""}>${escapeHtml(value)}</option>`;
}

function normalizeVisibility(value: string): Visibility {
  const normalized = value.toLowerCase();
  if (normalized === "public") return "Public";
  if (normalized === "unlisted") return "Unlisted";
  if (normalized === "direct") return "Direct";
  return "Followers";
}

function normalizeProtocol(value: string): ProtocolRoute {
  const normalized = value.toLowerCase();
  if (normalized === "atproto") return "AtProto";
  if (normalized === "both") return "Both";
  return "ActivityPub";
}

function sectionSubtitle(section: string) {
  if (section === "Compose") return "Private-by-default publishing with explicit public and E2EE modes";
  if (section === "Audience") return "Direct-audience lists with explicit sensitive-category boundaries";
  if (section === "Friends") return "Owner-only mutual relationships and friend feed";
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
