import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Visibility = "Public" | "Unlisted" | "Followers" | "Direct";
type ProtocolRoute = "ActivityPub" | "AtProto" | "Both";
type SensitiveCategory = "medical" | "adult" | "political" | "family-only" | "work-sensitive";

type OwnerAccountProfile = {
  id: string;
  label: string;
  instance_url: string;
  active: boolean;
  owner_token_present: boolean;
};

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

type DeletedPost = {
  ok: boolean;
  id: string;
  deleted: boolean;
  delivery_ids: string[];
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
  description?: string | null;
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
    canonical_url?: string | null;
    actor_id?: string | null;
    actor_handle?: string | null;
    actor_display_name?: string | null;
    content_html?: string | null;
    summary?: string | null;
    object_type?: string | null;
    published_at?: string | null;
    watch_type?: string | null;
    watch_target?: string | null;
    reply_target?: string | null;
    actions?: string[] | null;
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
    watch_type?: string | null;
    watch_target?: string | null;
    follow_target?: string | null;
    actions?: string[] | null;
  }>;
  provider_errors: Array<{
    provider: string;
    network: string;
    error: string;
  }>;
  public_search_guard: {
    blocked: boolean;
    requires_confirmation: boolean;
    confirmed: boolean;
    categories: string[];
    message?: string | null;
  };
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

const homeLaneOrder = ["Friends", "Following", "Mentions", "Watches", "Public", "Drafts/Saved"] as const;
type HomeLane = (typeof homeLaneOrder)[number];

const dailyQueueFilterOrder = ["All", "Unread", "Needs reply", "Blocked/hidden", "Protocol/source"] as const;
type DailyQueueFilter = (typeof dailyQueueFilterOrder)[number];

type DailyQueueItem = {
  id: string;
  label: string;
  count: number;
  detail: string;
  section: string;
  tags: DailyQueueFilter[];
  urgency: "high" | "medium" | "low";
};

type FeedPreset = {
  id: string;
  label: string;
  lane: HomeLane;
  description: string;
  why: string;
  privacy: string;
  ranking: string;
};

const feedPresets: FeedPreset[] = [
  {
    id: "friends-first",
    label: "Friends first",
    lane: "Friends",
    description: "Mutual relationships, chronological.",
    why: "Shown because the actor is both followed by you and approved as a follower.",
    privacy: "Feed membership is private unless explicitly shared.",
    ranking: "Chronological default"
  },
  {
    id: "following-only",
    label: "Following only",
    lane: "Following",
    description: "People you follow, without engagement ranking.",
    why: "Shown because you follow the actor from this Dais instance.",
    privacy: "Following membership stays operator-only by default.",
    ranking: "Chronological default"
  },
  {
    id: "watch-reader",
    label: "Watch reader",
    lane: "Watches",
    description: "Public sources harvested without sending follow requests.",
    why: "Shown because the operator added a private watch target.",
    privacy: "Watch membership is private and does not create a remote relationship.",
    ranking: "Chronological default"
  },
  {
    id: "public-search",
    label: "Public search",
    lane: "Public",
    description: "Public posts surfaced from searches and public lanes.",
    why: "Shown because it matched a public-source or public-search rule.",
    privacy: "Search presets are local until deliberately shared.",
    ranking: "Optional ranking off"
  },
  {
    id: "saved-research",
    label: "Science saved search",
    lane: "Drafts/Saved",
    description: "Saved search and bookmarked reading queue.",
    why: "Shown because it is saved by this operator, not because it is boosted.",
    privacy: "Saved searches are private unless explicitly shared.",
    ranking: "Chronological default"
  }
];

const sectionGroups = [
  {
    label: "Today",
    sections: ["Home", "Compose", "Search", "Discovery", "Notifications", "DMs"],
  },
  {
    label: "People",
    sections: ["Following", "Friends", "Followers", "Audience"],
  },
  {
    label: "Library",
    sections: ["Posts", "Sources", "Watches"],
  },
  {
    label: "Operate",
    sections: ["Moderation", "Deliveries", "Stats", "Profile", "Settings", "Diagnostics"],
  },
];
const sections = sectionGroups.flatMap((group) => group.sections);
const toolbarSections = ["Compose", "Search", "Discovery", "Following", "Watches"];

const smokeMode = new URLSearchParams(window.location.search).get("smoke") === "1";
const smokePostId = "https://social.dais.social/users/social/posts/smoke-post";

let snapshot: OwnerSnapshot | null = null;
let accountProfiles: OwnerAccountProfile[] = [];
let active = smokeSection() || "Home";
let notice = "";
let draftAttachments: string[] = smokeMode
  ? [JSON.stringify({ url: "https://social.dais.social/media/smoke-upload.png", mediaType: "image/png", name: "Smoke upload alt text" })]
  : [];
let draftReplyTo = "";
let draftMediaDescription = smokeMode ? "Smoke upload alt text" : "";
let discoveredActor: DiscoveredActor | null = null;
let selectedPostDetail: OwnerPostDetail | null = null;
let notifications: OwnerNotification[] = [];
let deliveries: OwnerDelivery[] = [];
let sourceSubscriptions: SourceSubscription[] = [];
let sourceItems: OwnerSnapshot["sources"] = [];
let watchSubscriptions: SourceSubscription[] = [];
let watchItems: OwnerSnapshot["sources"] = [];
let moderationState: ModerationState | null = null;
let moderationReplies: ModerationReply[] = [];
let directMessages: OwnerDirectMessage[] = [];
let searchQuery = "";
let searchScope = "local";
let searchProvider = "all";
let searchResultType = "all";
let searchServers = "";
let searchSort = "latest";
let searchLang = "";
let searchAuthor = "";
let searchTag = "";
let searchResults: OwnerSearchResult = emptySearchResults();
let ownerStats: OwnerStats | null = null;
let showTimelineReplies = false;
let showSourceItems = true;
let activeHomeLane: HomeLane = "Following";
let activeFeedPresetId = "following-only";
let selectedHomePostId = smokePostId;
let showHomeInspector = false;
let activeDailyQueueFilter: DailyQueueFilter = "All";
let dismissedDailyQueueItems: string[] = [];
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
    provider_errors: [],
    public_search_guard: {
      blocked: false,
      requires_confirmation: false,
      confirmed: false,
      categories: [],
      message: null
    }
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
  if (command === "owner_accounts") return smokeAccounts() as T;
  if (command === "switch_owner_account" || command === "delete_owner_account" || command === "save_owner_settings") {
    return undefined as T;
  }
  if (command === "owner_post_detail") return smokePostDetail(String(args?.objectId || smokePostId)) as T;
  if (command === "delete_owner_post") {
    return {
      ok: true,
      id: String(args?.objectId || smokePostId),
      deleted: true,
      delivery_ids: ["delivery-delete-smoke"]
    } as T;
  }
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
  if (command === "owner_watches") {
    return {
      subscriptions: [smokeWatchSubscription()],
      items: [smokeWatchItem()]
    } as T;
  }
  if (command === "add_owner_watch") {
    return {
      ok: true,
      source: {
        ...smokeWatchSubscription(),
        source_type: `watch_${String(args?.watchType || "rss")}`.replace("watch_activitypub_actor", "watch_activitypub_actor"),
        url: String(args?.target || "news.example")
      }
    } as T;
  }
  if (command === "refresh_owner_watch") {
    return { ok: true, items: [{ id: String(args?.id || "watch-smoke"), ok: true, status: "active" }] } as T;
  }
  if (command === "remove_owner_watch") return undefined as T;
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
      deliveries_failed: 1,
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
  if (command === "owner_notifications") return smokeNotifications() as T;
  if (command === "owner_deliveries") return smokeDeliveries() as T;
  if (command === "owner_direct_messages") return smokeDirectMessages() as T;
  if (command === "owner_diagnostics") return data.diagnostics as T;
  if (command === "revoke_owner_media") return { ok: true } as T;
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
    const scope = String(args?.scope || "local").toLowerCase();
    const query = String(args?.query || "");
    const includeLocal = scope !== "public" && scope !== "remote";
    const includePublic = scope === "public" || scope === "remote" || scope === "all";
    const publicCategories = includePublic ? sensitiveCategoriesForText(query) : [];
    const confirmedPublicSensitive = Boolean(args?.confirmPublicSensitive || args?.confirm_public_sensitive);
    const publicBlocked = includePublic && publicCategories.length > 0 && !confirmedPublicSensitive;
    return {
      posts: includeLocal ? data.posts.map((post) => ({
        id: post.id,
        content: post.content,
        visibility: String(post.visibility),
        protocol: String(post.protocol),
        published_at: post.published_at || null
      })) : [],
      users: includeLocal ? data.following.map((row) => ({
        actor_id: row.target_actor_id,
        relation: "following",
        status: row.status,
        created_at: row.created_at || null
      })) : [],
      sources: includeLocal ? [smokeSourceSubscription()] : [],
      source_items: includeLocal ? data.sources.map((item) => ({
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
      })) : [],
      public_posts: includePublic && !publicBlocked ? [
        {
          provider: "bluesky",
          network: "atproto",
          id: "at://did:plc:smoke/app.bsky.feed.post/3smoke",
          url: "https://bsky.app/profile/smoke.example/post/3smoke",
          content: "Public smoke result",
          actor_handle: "smoke.example",
          published_at: "2026-06-17T12:00:00Z",
          watch_type: "bluesky_post",
          watch_target: "at://did:plc:smoke/app.bsky.feed.post/3smoke",
          reply_target: "at://did:plc:smoke/app.bsky.feed.post/3smoke",
          actions: ["open", "watch", "reply"]
        }
      ] : [],
      public_actors: includePublic && !publicBlocked ? [
        {
          provider: "mastodon.social",
          network: "activitypub",
          id: "https://mastodon.social/@smoke",
          handle: "smoke@mastodon.social",
          display_name: "Smoke",
          url: "https://mastodon.social/@smoke",
          watch_type: "activitypub_actor",
          watch_target: "https://mastodon.social/@smoke",
          follow_target: "https://mastodon.social/@smoke",
          actions: ["watch", "follow", "open"]
        }
      ] : [],
      provider_errors: [],
      public_search_guard: {
        blocked: publicBlocked,
        requires_confirmation: includePublic && publicCategories.length > 0,
        confirmed: includePublic && publicCategories.length > 0 && confirmedPublicSensitive,
        categories: publicCategories,
        message: publicBlocked
          ? "Public provider search skipped until the operator confirms this sensitive query."
          : null
      }
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
  if (command === "follow_actor" || command === "unfollow_actor") {
    return {
      ok: true,
      following: {
        id: "following-smoke",
        actor_id: "https://social.dais.social/users/social",
        target_actor_id: String(args?.target || "https://mastodon.example/users/alice"),
        target_inbox: "https://mastodon.example/inbox",
        status: command === "follow_actor" ? "requested" : "removed",
        accepted_at: null
      },
      delivery_ids: ["delivery-follow-smoke"]
    } as T;
  }
  if (command === "block_owner_actor") return { ok: true, actor_id: String(args?.actorId || args?.actor_id || "") } as T;
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
      },
      {
        id: "follower-pending-smoke",
        actor_id: "https://social.dais.social/users/social",
        follower_actor_id: "https://mastodon.example/users/bob",
        follower_inbox: "https://mastodon.example/inbox",
        status: "pending"
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

function smokeAccounts(): OwnerAccountProfile[] {
  return [
    {
      id: "account-social-dais-social",
      label: "Dais Social",
      instance_url: "https://social.dais.social",
      active: true,
      owner_token_present: true
    },
    {
      id: "account-skeptical-engineer",
      label: "Skeptical Engineer",
      instance_url: "https://skeptical.engineer",
      active: false,
      owner_token_present: false
    },
    {
      id: "account-joneslaw-io",
      label: "Jones Law",
      instance_url: "https://joneslaw.io",
      active: false,
      owner_token_present: false
    }
  ];
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

function smokeWatchSubscription(): SourceSubscription {
  return {
    id: "watch-smoke",
    source_type: "watch_bluesky_actor",
    url: "nasa.gov",
    title: "NASA on Bluesky",
    status: "active",
    refresh_cadence_minutes: 60,
    error_count: 0,
    policy_json: "{\"watch\":true,\"public_only\":true,\"no_remote_relationship\":true}"
  };
}

function smokeWatchItem(): OwnerSnapshot["sources"][number] {
  return {
    id: "watch-item-smoke",
    title: "Public launch update",
    source_type: "watch_bluesky_actor",
    canonical_url: "https://bsky.app/profile/nasa.gov/post/3smoke",
    excerpt: "A public post harvested into the private watch reader.",
    read: false
  };
}

function smokeNotifications(): OwnerNotification[] {
  return [
    {
      id: "notification-mention-smoke",
      type: "mention",
      actor_id: "https://mastodon.example/users/alice",
      actor_display_name: "Alice Example",
      post_id: smokePostId,
      activity_id: `${smokePostId}#mention`,
      content: "Alice mentioned you in a public thread.",
      read: true,
      created_at: "2026-06-16T14:08:00Z"
    },
    {
      id: "notification-reply-smoke",
      type: "reply",
      actor_id: "https://mastodon.example/users/alice",
      actor_display_name: "Alice Example",
      post_id: smokePostId,
      activity_id: `${smokePostId}#reply`,
      content: "Alice replied and may need a response.",
      read: false,
      created_at: "2026-06-16T14:09:00Z"
    }
  ];
}

function smokeDeliveries(): OwnerDelivery[] {
  return [
    {
      id: "delivery-failed-smoke",
      post_id: smokePostId,
      target_type: "shared_inbox",
      target_url: "https://mastodon.example/inbox",
      protocol: "ActivityPub",
      status: "failed",
      retry_count: 2,
      last_attempt_at: "2026-06-16T14:10:00Z",
      error_message: "Smoke delivery failure for daily queue review.",
      activity_type: "Create",
      created_at: "2026-06-16T14:00:00Z"
    }
  ];
}

function smokeDirectMessages(): OwnerDirectMessage[] {
  return [
    {
      id: "dm-smoke",
      conversation_id: "conversation-alice-smoke",
      sender_id: "https://mastodon.example/users/alice",
      content: "Private smoke DM that belongs in the daily queue.",
      published_at: "2026-06-16T14:11:00Z"
    }
  ];
}

async function load() {
  render();
  accountProfiles = await ownerInvoke<OwnerAccountProfile[]>("owner_accounts");
  snapshot = await ownerInvoke<OwnerSnapshot>("owner_snapshot");
  if (!accountProfiles.length) {
    accountProfiles = [{
      id: "active",
      label: snapshot.profile.display_name || shortHost(snapshot.settings.instance_url),
      instance_url: snapshot.settings.instance_url,
      active: true,
      owner_token_present: snapshot.settings.owner_token_present
    }];
  }
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
    <main class="shell mac-shell">
      <aside class="sidebar source-list">
        <div class="brand account-card">
          <div class="mark">${escapeHtml((snapshot.profile.username || "d").slice(0, 1).toLowerCase())}</div>
          <div>
            <strong>${escapeHtml(snapshot.profile.display_name || "dais owner")}</strong>
            <span>${escapeHtml(snapshot.profile.public_handle || shortHost(snapshot.settings.instance_url))}</span>
            <span>${escapeHtml(shortHost(snapshot.settings.instance_url))}</span>
            ${accountSwitcher()}
          </div>
        </div>
        <nav class="nav source-nav">${sectionGroups.map(navGroup).join("")}</nav>
      </aside>
      <section class="workspace">
        <header class="topbar toolbar">
          <div class="title-stack">
            <span class="breadcrumb">${escapeHtml(sectionGroupLabel(active))}</span>
            <h1>${escapeHtml(active)}</h1>
            <p>${escapeHtml(sectionSubtitle(active))}</p>
          </div>
          <div class="toolbar-actions">
            ${toolbarSections.map((section) => toolbarButton(section)).join("")}
          </div>
          <div class="top-actions status-cluster">
            <span class="pill ${snapshot.settings.default_visibility === "Public" ? "warn" : "ok"}">${escapeHtml(audienceLabel(snapshot.settings.default_visibility))}</span>
            <span class="pill">${escapeHtml(snapshot.settings.default_protocol)}</span>
            <span class="pill ${snapshot.settings.owner_token_present ? "ok" : "warn"}">${
              snapshot.settings.owner_token_present ? "Token stored" : "Token needed"
            }</span>
            <span class="pill ${apiDiagnostic?.ok ? "ok" : "warn"}">${
              apiDiagnostic?.ok ? "Owner API live" : "Local preview"
            }</span>
          </div>
        </header>
        <div class="workspace-scroll">
          ${notice ? `<div class="notice">${escapeHtml(notice)}</div>` : ""}
          ${view(active, snapshot)}
        </div>
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
  app.querySelectorAll<HTMLButtonElement>("[data-home-lane]").forEach((button) => {
    button.addEventListener("click", () => {
      const lane = button.dataset.homeLane || "";
      if (isHomeLane(lane)) {
        activeHomeLane = lane;
        render();
      }
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-feed-preset]").forEach((button) => {
    button.addEventListener("click", () => {
      applyFeedPreset(button.dataset.feedPreset || "");
      render();
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-home-select]").forEach((button) => {
    button.addEventListener("click", () => {
      selectedHomePostId = button.dataset.homeSelect || selectedHomePostId;
      showHomeInspector = true;
      render();
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-home-hide-inspector]").forEach((button) => {
    button.addEventListener("click", () => {
      showHomeInspector = false;
      render();
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-daily-filter]").forEach((button) => {
    button.addEventListener("click", () => {
      const filter = button.dataset.dailyFilter || "";
      if (isDailyQueueFilter(filter)) {
        activeDailyQueueFilter = filter;
        render();
      }
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-daily-done]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.dailyDone || "";
      if (id && !dismissedDailyQueueItems.includes(id)) {
        dismissedDailyQueueItems = [...dismissedDailyQueueItems, id];
      }
      render();
    });
  });
  app.querySelector<HTMLFormElement>("#settings-form")?.addEventListener("submit", saveSettings);
  app.querySelector<HTMLSelectElement>("#account-switcher")?.addEventListener("change", (event) => {
    void switchAccount((event.currentTarget as HTMLSelectElement).value);
  });
  app.querySelectorAll<HTMLButtonElement>("[data-account-switch]").forEach((button) => {
    button.addEventListener("click", () => {
      void switchAccount(button.dataset.accountSwitch || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-account-delete]").forEach((button) => {
    button.addEventListener("click", () => {
      void deleteAccount(button.dataset.accountDelete || "");
    });
  });
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
  app.querySelectorAll<HTMLButtonElement>("[data-revoke-attachment]").forEach((button) => {
    button.addEventListener("click", () => {
      void revokeDraftAttachment(button.dataset.revokeAttachment || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-revoke-media]").forEach((button) => {
    button.addEventListener("click", () => {
      void revokeMediaUrl(button.dataset.revokeMedia || "");
    });
  });
  app.querySelector<HTMLFormElement>("#follow-form")?.addEventListener("submit", followActor);
  app.querySelector<HTMLFormElement>("#discover-form")?.addEventListener("submit", discoverActor);
  app.querySelector<HTMLFormElement>("#source-form")?.addEventListener("submit", addSource);
  app.querySelector<HTMLFormElement>("#watch-form")?.addEventListener("submit", addWatch);
  app.querySelector<HTMLFormElement>("#search-form")?.addEventListener("submit", runSearch);
  app.querySelector<HTMLButtonElement>("[data-confirm-public-search]")?.addEventListener("click", () => {
    void executeSearch(searchQuery, searchScope, true);
  });
  app.querySelector<HTMLFormElement>("#block-actor-form")?.addEventListener("submit", blockActor);
  app.querySelector<HTMLFormElement>("#block-domain-form")?.addEventListener("submit", blockDomain);
  app.querySelector<HTMLFormElement>("#allow-host-form")?.addEventListener("submit", allowHost);
  app.querySelector<HTMLFormElement>("#audience-list-form")?.addEventListener("submit", saveAudienceList);
  app.querySelector<HTMLFormElement>("#moderation-settings-form")?.addEventListener("submit", saveModerationSettings);
  app.querySelector<HTMLButtonElement>("[data-refresh-sources]")?.addEventListener("click", () => {
    void refreshSource(null);
  });
  app.querySelector<HTMLButtonElement>("[data-refresh-watches]")?.addEventListener("click", () => {
    void refreshWatch(null);
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
  app.querySelectorAll<HTMLButtonElement>("[data-delete-post]").forEach((button) => {
    button.addEventListener("click", () => {
      void deletePost(button.dataset.deletePost || "");
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
  app.querySelectorAll<HTMLButtonElement>("[data-search-watch]").forEach((button) => {
    button.addEventListener("click", () => {
      void addWatchTarget(
        button.dataset.watchType || "",
        button.dataset.watchTarget || "",
        button.dataset.watchTitle || ""
      );
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-search-reply]").forEach((button) => {
    button.addEventListener("click", () => {
      const target = button.dataset.searchReply || "";
      if (!target) return;
      draftReplyTo = target;
      active = "Compose";
      notice = `Replying to ${shortUrl(target)}.`;
      render();
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-search-follow]").forEach((button) => {
    button.addEventListener("click", () => {
      void followTarget(button.dataset.searchFollow || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-home-block]").forEach((button) => {
    button.addEventListener("click", () => {
      void blockActorTarget(button.dataset.homeBlock || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-search-interaction]").forEach((button) => {
    button.addEventListener("click", () => {
      void ownerInteraction(button.dataset.object || "", button.dataset.searchInteraction || "");
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
  app.querySelectorAll<HTMLButtonElement>("[data-watch-refresh]").forEach((button) => {
    button.addEventListener("click", () => {
      void refreshWatch(button.dataset.watchRefresh || "");
    });
  });
  app.querySelectorAll<HTMLButtonElement>("[data-watch-remove]").forEach((button) => {
    button.addEventListener("click", () => {
      void removeWatch(button.dataset.watchRemove || "");
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
  return `<button class="${section === active ? "active" : ""}" data-section="${escapeAttr(section)}">
    <span>${navGlyph(section)}</span>
    <span>${escapeHtml(section)}</span>
    ${navBadge(section)}
  </button>`;
}

function navGroup(group: { label: string; sections: string[] }) {
  return `<section class="nav-group">
    <h2>${escapeHtml(group.label)}</h2>
    ${group.sections.map(navButton).join("")}
  </section>`;
}

function accountSwitcher() {
  if (!accountProfiles.length) return "";
  return `<select id="account-switcher" class="account-switcher" aria-label="Active Dais account">
    ${accountProfiles
      .map((account) => `<option value="${escapeAttr(account.id)}"${account.active ? " selected" : ""}>${escapeHtml(account.label)}</option>`)
      .join("")}
  </select>`;
}

function activeAccountProfile() {
  return accountProfiles.find((account) => account.active) || accountProfiles[0] || null;
}

function toolbarButton(section: string) {
  return `<button type="button" class="${section === active ? "active" : ""}" data-section="${escapeAttr(section)}">
    <span>${navGlyph(section)}</span>${escapeHtml(toolbarLabel(section))}
  </button>`;
}

function toolbarLabel(section: string) {
  if (section === "Compose") return "New";
  if (section === "Discovery") return "Find";
  if (section === "Following") return "Follow";
  if (section === "Watches") return "Watch";
  return section;
}

function navBadge(section: string) {
  if (!snapshot) return "";
  if (section === "Notifications") {
    const unread = ownerStats?.notifications_unread || notifications.filter((row) => !isRead(row.read)).length;
    return unread ? `<strong>${escapeHtml(String(unread))}</strong>` : "";
  }
  if (section === "Followers") {
    const pending = snapshot.followers.filter((row) => row.status === "pending").length;
    return pending ? `<strong>${escapeHtml(String(pending))}</strong>` : "";
  }
  if (section === "Moderation") {
    const queue = (moderationState || snapshot.moderation).reply_queue_count || 0;
    return queue ? `<strong>${escapeHtml(String(queue))}</strong>` : "";
  }
  return "";
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
    case "Watches":
      return watchesView(data);
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
        <select name="provider">
          <option value="all"${searchProvider === "all" ? " selected" : ""}>All providers</option>
          <option value="bluesky"${searchProvider === "bluesky" ? " selected" : ""}>Bluesky</option>
          <option value="activitypub"${searchProvider === "activitypub" ? " selected" : ""}>ActivityPub</option>
        </select>
        <select name="result_type">
          <option value="all"${searchResultType === "all" ? " selected" : ""}>Posts + actors</option>
          <option value="posts"${searchResultType === "posts" ? " selected" : ""}>Posts</option>
          <option value="actors"${searchResultType === "actors" ? " selected" : ""}>Actors</option>
        </select>
        <select name="sort">
          <option value="latest"${searchSort === "latest" ? " selected" : ""}>Latest</option>
          <option value="top"${searchSort === "top" ? " selected" : ""}>Top</option>
        </select>
        <input name="servers" value="${escapeAttr(searchServers)}" placeholder="ActivityPub servers" />
        <input name="author" value="${escapeAttr(searchAuthor)}" placeholder="Author" />
        <input name="tag" value="${escapeAttr(searchTag)}" placeholder="Tag" />
        <input name="lang" value="${escapeAttr(searchLang)}" placeholder="Lang" />
        <button type="submit">Search</button>
      </form>
      ${searchPublicGuardHtml(searchResults.public_search_guard)}
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

function searchPublicGuardHtml(guard: OwnerSearchResult["public_search_guard"]) {
  if (!guard || (!guard.blocked && !guard.requires_confirmation && !(guard.categories || []).length)) {
    return "";
  }
  const categories = guard.categories || [];
  return `<div class="privacy-note">
    <strong>${guard.blocked ? "Public search paused" : "Public search confirmed"}</strong>
    ${guard.message ? `<p>${escapeHtml(guard.message)}</p>` : ""}
    ${categories.length ? `<div class="sensitivity-tags">${categories.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("")}</div>` : ""}
    ${guard.blocked ? `<button type="button" data-confirm-public-search>Search public providers</button>` : ""}
  </div>`;
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

function watchesView(data: OwnerSnapshot) {
  const items = watchItems.length ? watchItems : data.sources.filter((item) => item.source_type.startsWith("watch_"));
  return `<section class="split">
    <article>
      <div class="section-heading">
        <h2 class="section-label">Watches</h2>
        <button type="button" data-refresh-watches>Refresh all</button>
      </div>
      <form id="watch-form" class="compact-form">
        <select name="watch_type">
          <option value="rss">RSS feed</option>
          <option value="atom">Atom feed</option>
          <option value="activitypub_actor">ActivityPub actor</option>
          <option value="activitypub_object">ActivityPub post</option>
          <option value="bluesky_actor">Bluesky actor</option>
          <option value="bluesky_post">Bluesky post</option>
        </select>
        <input name="target" placeholder="@user@server, bsky handle, or public URL" />
        <input name="title" placeholder="Label" />
        <input name="cadence_minutes" type="number" min="5" max="1440" value="60" />
        <button type="submit">Watch</button>
      </form>
      ${list(watchSubscriptions.map(watchSubscriptionCard), "No watch targets returned by the owner API.")}
    </article>
    <article>
      <h2 class="section-label">Harvested public posts</h2>
      ${list(items.map(sourceCard), "No watched public posts are available yet.")}
    </article>
  </section>`;
}

function dashboardView(data: OwnerSnapshot) {
  return `
    <section class="home-grid">
      <article class="panel feed-pane home-feed">
        <div class="section-heading home-heading">
          <div>
            <h2>Daily social home</h2>
            <p>${escapeHtml(activeHomeSummary())}</p>
          </div>
          <div class="home-header-actions">
            <span class="pill">${homeLaneCount(data, activeHomeLane)} item${homeLaneCount(data, activeHomeLane) === 1 ? "" : "s"}</span>
            ${homeDisplayMenu(data)}
          </div>
        </div>
        ${homeContextBar(data)}
        ${homeLaneContent(data)}
      </article>
      <aside class="workflow-side">
        ${dailyQueueView(data)}
        ${showHomeInspector ? homeInspectorView(data) : homeSelectionHint(data)}
        ${privacyStatusView(data)}
      </aside>
    </section>
    <section class="overview-grid">
      ${metric("Posts", data.posts.length, "recent local posts", "Posts")}
      ${metric("Followers", data.followers.filter((row) => row.status === "approved").length, "approved followers", "Followers")}
      ${metric("Friends", data.friends.length, "mutual relationships", "Friends")}
      ${metric("Following", data.following.length, "private graph", "Following")}
      ${metric("Reader", data.sources.length, "source items", "Sources")}
    </section>`;
}

function activeHomeSummary() {
  if (activeHomeLane === "Friends") return "Mutual friends first, without exposing your graph publicly.";
  if (activeHomeLane === "Mentions") return "Mentions, replies, and DMs that may need a response.";
  if (activeHomeLane === "Watches") return "Public sources watched privately, with no follow request sent.";
  if (activeHomeLane === "Public") return "Public posts and search results, separate from private relationships.";
  if (activeHomeLane === "Drafts/Saved") return "Saved reading and drafts kept local unless deliberately shared.";
  return "People you follow, sorted chronologically and kept private by default.";
}

function homeContextBar(data: OwnerSnapshot) {
  const activePreset = feedPresets.find((preset) => preset.id === activeFeedPresetId) || feedPresets[0];
  return `<section class="home-context-bar" aria-label="Current feed view">
    <div>
      <span>${escapeHtml(activePreset.label)}</span>
      <strong>${escapeHtml(activeHomeLane)}</strong>
    </div>
    <div>
      <span>${showTimelineReplies ? "Replies included" : "Top-level posts"}</span>
      <strong>${escapeHtml(activePreset.ranking)}</strong>
    </div>
    <div>
      <span>Privacy</span>
      <strong>${escapeHtml(activePreset.privacy)}</strong>
    </div>
  </section>`;
}

function homeDisplayMenu(data: OwnerSnapshot) {
  const activePreset = feedPresets.find((preset) => preset.id === activeFeedPresetId) || feedPresets[0];
  return `<details class="control-menu home-display-menu">
    <summary>Display</summary>
    <div class="control-menu-panel">
      <section>
        <h3>Feed presets</h3>
        <p>${escapeHtml(activePreset.why)}</p>
        <div class="feed-presets compact">
          ${feedPresets
            .map((preset) => `<button type="button" class="preset-button ${preset.id === activeFeedPresetId ? "active" : ""}" data-feed-preset="${escapeAttr(preset.id)}">
              <strong>${escapeHtml(preset.label)}</strong>
              <span>${escapeHtml(preset.description)}</span>
            </button>`)
            .join("")}
        </div>
        <small>Saved searches are private unless explicitly shared.</small>
      </section>
      <section>
        <h3>Feed lanes</h3>
        <div class="lane-tabs compact">
          ${homeLaneOrder
            .map((lane) => `<button type="button" class="lane-button ${lane === activeHomeLane ? "active" : ""}" data-home-lane="${escapeAttr(lane)}">
              <strong>${escapeHtml(lane)}</strong>
              <span>${homeLaneCount(data, lane)}</span>
            </button>`)
            .join("")}
        </div>
      </section>
      <label class="menu-check"><input type="checkbox" data-feed-toggle="replies" ${showTimelineReplies ? "checked" : ""} /> Show replies in feed</label>
      <p class="menu-note">Chronological, not engagement ranked. Filtering changes only what this client displays.</p>
    </div>
  </details>`;
}

function feedPresetView() {
  const activePreset = feedPresets.find((preset) => preset.id === activeFeedPresetId) || feedPresets[0];
  return `<section class="feed-preset-panel" aria-label="Feed presets">
    <div class="section-heading">
      <h2>Feed presets</h2>
      <span class="pill">${escapeHtml(activePreset.ranking)}</span>
    </div>
    <div class="feed-presets">
      ${feedPresets
        .map((preset) => `<button type="button" class="preset-button ${preset.id === activeFeedPresetId ? "active" : ""}" data-feed-preset="${escapeAttr(preset.id)}">
          <strong>${escapeHtml(preset.label)}</strong>
          <span>${escapeHtml(preset.description)}</span>
        </button>`)
        .join("")}
    </div>
    <div class="preset-detail">
      <strong>Why this item appears</strong>
      <p>${escapeHtml(activePreset.why)}</p>
      <span>${escapeHtml(activePreset.privacy)}</span>
      <span>Saved searches are private unless explicitly shared.</span>
    </div>
  </section>`;
}

function homeLaneControls(data: OwnerSnapshot) {
  return `<section class="home-lanes" aria-label="Feed lanes">
    <div class="section-heading">
      <h2>Feed lanes</h2>
      <span>Chronological, not engagement ranked</span>
    </div>
    <div class="lane-tabs">
      ${homeLaneOrder
        .map((lane) => `<button type="button" class="lane-button ${lane === activeHomeLane ? "active" : ""}" data-home-lane="${escapeAttr(lane)}">
          <strong>${escapeHtml(lane)}</strong>
          <span>${homeLaneCount(data, lane)}</span>
        </button>`)
        .join("")}
    </div>
  </section>`;
}

function homeLaneContent(data: OwnerSnapshot) {
  if (activeHomeLane === "Friends") {
    const friendActors = new Set(data.friends.map((row) => row.friend_actor_id));
    const rows = feedTimeline(data).filter((post) => friendActors.has(post.actor_id));
    return list(rows.map((post) => homeTimelineCard(post, data)), "No friend posts yet. Mutual friends appear here without exposing your graph publicly.");
  }
  if (activeHomeLane === "Following") {
    return list(feedTimeline(data).slice(0, 12).map((post) => homeTimelineCard(post, data)), "No followed posts yet. Follow people or sources to build this feed.");
  }
  if (activeHomeLane === "Mentions") {
    return list(notifications.map(notificationCard), "No mentions or reply notifications are waiting.");
  }
  if (activeHomeLane === "Watches") {
    const items = watchItems.length ? watchItems : data.sources;
    return list(items.map(sourceCard), "No watched public posts are available yet.");
  }
  if (activeHomeLane === "Public") {
    const rows = feedTimeline(data).filter((post) => String(post.visibility || "").toLowerCase() === "public");
    return list(rows.map((post) => homeTimelineCard(post, data)), "No public lane items yet. Run public search or add public watches.");
  }
  return list(data.posts.map(postCard), "No saved posts or drafts are available yet.");
}

function homeLaneCount(data: OwnerSnapshot, lane: HomeLane) {
  if (lane === "Friends") {
    const friendActors = new Set(data.friends.map((row) => row.friend_actor_id));
    return feedTimeline(data).filter((post) => friendActors.has(post.actor_id)).length;
  }
  if (lane === "Following") return feedTimeline(data).length;
  if (lane === "Mentions") return notifications.length || ownerStats?.notifications_unread || 0;
  if (lane === "Watches") return (watchItems.length ? watchItems : data.sources).length;
  if (lane === "Public") return feedTimeline(data).filter((post) => String(post.visibility || "").toLowerCase() === "public").length;
  return data.posts.length;
}

function homeTimelineCard(post: OwnerSnapshot["home_timeline"][number], data: OwnerSnapshot) {
  const author = post.actor_display_name || post.actor_username || actorLabel(post.actor_id);
  const selected = post.object_id === selectedHomePostId;
  const followButton = canFollowActor(data, post.actor_id)
    ? `<button type="button" data-search-follow="${escapeAttr(post.actor_id)}">Follow</button>`
    : "";
  return `<article class="item timeline home-timeline-card ${selected ? "selected" : ""}">
    <div>
      <h2>${escapeHtml(author)}</h2>
      ${postBodyHtml(post.content, post.content_html)}
      ${sensitivityBadgesHtml(post.content)}
    </div>
    <footer>
      <span title="${escapeAttr(audienceDescription(post.visibility))}">${escapeHtml(compactAudienceLabel(post.visibility))}</span>
      ${post.protocol ? `<span>${escapeHtml(post.protocol)}</span>` : ""}
      ${post.in_reply_to ? "<span>reply</span>" : ""}
      ${post.published_at ? `<time title="${escapeAttr(formatTime(post.published_at))}">${escapeHtml(formatFeedTime(post.published_at))}</time>` : ""}
      ${interactionCounts(post)}
      <button type="button" data-home-select="${escapeAttr(post.object_id)}">Details</button>
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.object_id)}">Reply</button>
      <details class="action-menu">
        <summary>More</summary>
        <div>
          <button type="button" data-timeline-action="like" data-object="${escapeAttr(post.object_id)}">Like/Favorite</button>
          <button type="button" data-timeline-action="boost" data-object="${escapeAttr(post.object_id)}">Boost/Repost</button>
          <button type="button" data-timeline-action="bookmark" data-object="${escapeAttr(post.object_id)}">Bookmark</button>
          <button type="button" data-search-watch data-watch-type="activitypub_object" data-watch-target="${escapeAttr(post.object_id)}" data-watch-title="${escapeAttr(author)}">Watch</button>
          ${followButton}
          <button type="button" data-home-block="${escapeAttr(post.actor_id)}">Mute/Block</button>
        </div>
      </details>
    </footer>
  </article>`;
}

function homeSelectionHint(data: OwnerSnapshot) {
  const post = selectedHomePost(data);
  if (!post) {
    return `<article class="panel home-inspector collapsed">
      <h2>Post details</h2>
      <p>Select Details on a post to inspect thread, relationship, visibility, and moderation context.</p>
    </article>`;
  }
  const author = post.actor_display_name || post.actor_username || actorLabel(post.actor_id);
  return `<article class="panel home-inspector collapsed">
    <div class="section-heading">
      <div>
        <h2>Post details</h2>
        <p>${escapeHtml(author)} - ${escapeHtml(audienceLabel(post.visibility))}</p>
      </div>
      <button type="button" data-home-select="${escapeAttr(post.object_id)}">Open</button>
    </div>
  </article>`;
}

function homeInspectorView(data: OwnerSnapshot) {
  const post = selectedHomePost(data);
  if (!post) {
    return `<article class="panel home-inspector">
      <h2>Post inspector</h2>
      <p>Select a feed item to inspect thread, relationship, visibility, and moderation context without navigating away.</p>
    </article>`;
  }
  const author = post.actor_display_name || post.actor_username || actorLabel(post.actor_id);
  const followButton = canFollowActor(data, post.actor_id)
    ? `<button type="button" data-search-follow="${escapeAttr(post.actor_id)}">Follow</button>`
    : "";
  return `<article class="panel home-inspector">
    <div class="section-heading">
      <h2>Post inspector</h2>
      <div class="detail-actions">
        <span class="pill">${escapeHtml(shortUrl(post.object_id))}</span>
        <button type="button" data-home-hide-inspector>Hide</button>
      </div>
    </div>
    ${postBodyHtml(post.content, post.content_html)}
    <dl>
      <dt>Thread context</dt><dd>${post.in_reply_to ? `Reply to ${escapeHtml(shortUrl(post.in_reply_to))}` : "Top-level post"} with ${post.reply_count || 0} repl${(post.reply_count || 0) === 1 ? "y" : "ies"}</dd>
      <dt>Relationship context</dt><dd>${escapeHtml(relationshipForActor(data, post.actor_id))}</dd>
      <dt>Visibility</dt><dd>${escapeHtml(audienceLabel(post.visibility))}</dd>
      <dt>Protocol/source</dt><dd>${escapeHtml(post.protocol || "unknown")} from ${escapeHtml(author)}</dd>
      <dt>Moderation context</dt><dd>${escapeHtml((moderationState || data.moderation).reply_policy || "warn")} replies, ${(moderationState || data.moderation).reply_queue_count || 0} waiting for review</dd>
    </dl>
    <div class="detail-actions">
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.object_id)}">Reply</button>
      <details class="action-menu">
        <summary>More actions</summary>
        <div>
          <button type="button" data-timeline-action="like" data-object="${escapeAttr(post.object_id)}">Like/Favorite</button>
          <button type="button" data-timeline-action="boost" data-object="${escapeAttr(post.object_id)}">Boost/Repost</button>
          <button type="button" data-timeline-action="bookmark" data-object="${escapeAttr(post.object_id)}">Bookmark</button>
          <button type="button" data-search-watch data-watch-type="activitypub_object" data-watch-target="${escapeAttr(post.object_id)}" data-watch-title="${escapeAttr(author)}">Watch</button>
          ${followButton}
          <button type="button" data-home-block="${escapeAttr(post.actor_id)}">Mute/Block</button>
        </div>
      </details>
    </div>
    <p class="privacy-note">The inspector keeps thread context beside the daily queue, so marking an item done/read does not lose the post being reviewed.</p>
  </article>`;
}

function selectedHomePost(data: OwnerSnapshot) {
  return data.home_timeline.find((post) => post.object_id === selectedHomePostId) || data.home_timeline[0] || null;
}

function relationshipForActor(data: OwnerSnapshot, actorId: string) {
  if (data.friends.some((row) => row.friend_actor_id === actorId)) return "Friend - mutual approved relationship";
  if (data.following.some((row) => row.target_actor_id === actorId)) return "Following - private graph";
  if (data.followers.some((row) => row.follower_actor_id === actorId && row.status === "approved")) return "Follower - approved";
  return "Public or unknown actor";
}

function canFollowActor(data: OwnerSnapshot, actorId: string) {
  return !data.following.some((row) => {
    if (row.target_actor_id !== actorId) return false;
    return followingStatusCanUnfollow(row.status);
  });
}

function dailyQueueView(data: OwnerSnapshot) {
  const items = filteredDailyQueueItems(data);
  const total = dailyAttentionCount(data);
  const visibleItems = activeDailyQueueFilter === "All" ? items.filter((item) => item.count > 0 || item.urgency !== "low") : items;
  return `<article class="panel daily-queue">
    <div class="section-heading">
      <div>
        <h2>Daily queue</h2>
        <p>Open items only by default. Use Filter for the full queue.</p>
      </div>
      <div class="home-header-actions">
        <span class="pill ${total ? "warn" : "ok"}">${total} open</span>
        <details class="control-menu queue-menu">
          <summary>Filter</summary>
          <div class="control-menu-panel">
            <h3>Daily queue filters</h3>
            <div class="queue-filters">
              ${dailyQueueFilterOrder.map((filter) => `<button type="button" class="${filter === activeDailyQueueFilter ? "active" : ""}" data-daily-filter="${escapeAttr(filter)}">${escapeHtml(filter)}</button>`).join("")}
            </div>
          </div>
        </details>
      </div>
    </div>
    <section class="queue-list">
      ${visibleItems.map(dailyQueueCard).join("") || `<article class="empty"><p>No open items match this queue filter.</p></article>`}
    </section>
  </article>`;
}

function dailyQueueItems(data: OwnerSnapshot): DailyQueueItem[] {
  const unreadNotifications = notifications.length
    ? notifications.filter((row) => !isRead(row.read)).length
    : ownerStats?.notifications_unread || 0;
  const pendingFollowers = data.followers.filter((row) => row.status === "pending").length;
  const replyQueue = (moderationState || data.moderation).reply_queue_count || 0;
  const flaggedReplies = (moderationState || data.moderation).flagged_reply_count || 0;
  const failedDeliveries = deliveries.length
    ? deliveries.filter((row) => row.status === "failed").length
    : ownerStats?.deliveries_failed || 0;
  return [
    {
      id: "mentions-replies",
      label: "Mentions and replies",
      count: unreadNotifications,
      detail: "Unread mentions and reply notifications that may need a response.",
      section: "Notifications",
      tags: ["Unread", "Needs reply", "Protocol/source"],
      urgency: unreadNotifications ? "high" : "low"
    },
    {
      id: "direct-messages",
      label: "Direct messages",
      count: directMessages.length,
      detail: "Private DMs stay out of public and friends feeds.",
      section: "DMs",
      tags: ["Unread", "Needs reply"],
      urgency: directMessages.length ? "high" : "low"
    },
    {
      id: "follow-requests",
      label: "Follow requests",
      count: pendingFollowers,
      detail: "Review who can enter follower-only audiences.",
      section: "Followers",
      tags: ["Unread", "Protocol/source"],
      urgency: pendingFollowers ? "medium" : "low"
    },
    {
      id: "moderation-replies",
      label: "Review replies",
      count: replyQueue,
      detail: `${flaggedReplies} blocked/hidden or sensitive repl${flaggedReplies === 1 ? "y" : "ies"} need moderation review.`,
      section: "Moderation",
      tags: ["Needs reply", "Blocked/hidden"],
      urgency: replyQueue ? "high" : "low"
    },
    {
      id: "delivery-failures",
      label: "Delivery failures",
      count: failedDeliveries,
      detail: "Protocol delivery failures that may explain missing replies or follows.",
      section: "Deliveries",
      tags: ["Protocol/source"],
      urgency: failedDeliveries ? "medium" : "low"
    }
  ];
}

function filteredDailyQueueItems(data: OwnerSnapshot) {
  const items = dailyQueueItems(data);
  if (activeDailyQueueFilter === "All") return items;
  return items.filter((item) => item.tags.includes(activeDailyQueueFilter));
}

function dailyAttentionCount(data: OwnerSnapshot) {
  return dailyQueueItems(data)
    .filter((item) => !dismissedDailyQueueItems.includes(item.id))
    .reduce((sum, item) => sum + item.count, 0);
}

function dailyQueueCard(item: DailyQueueItem) {
  const done = dismissedDailyQueueItems.includes(item.id);
  return `<article class="queue-item ${item.urgency} ${done ? "done" : ""}">
    <div>
      <strong>${escapeHtml(item.label)}</strong>
      <p>${escapeHtml(item.detail)}</p>
    </div>
    <span class="queue-count">${done ? "done" : escapeHtml(String(item.count))}</span>
    <footer>
      ${item.tags.map((tag) => `<span>${escapeHtml(tag)}</span>`).join("")}
      <button type="button" data-section="${escapeAttr(item.section)}">Open</button>
      ${done ? `<span>Marked done/read</span>` : `<button type="button" data-daily-done="${escapeAttr(item.id)}">Mark done/read</button>`}
    </footer>
  </article>`;
}

function quickActionsView(data: OwnerSnapshot) {
  const pendingFollowers = data.followers.filter((row) => row.status === "pending").length;
  const queuedReplies = (moderationState || data.moderation).reply_queue_count || 0;
  return `<article class="panel quick-actions">
    <div>
      <h2>Common workflows</h2>
      <p>Most work starts with reading, replying, finding people, or watching public posts privately.</p>
    </div>
    <div class="action-grid">
      <button type="button" data-section="Search">Search public posts</button>
      <button type="button" data-section="Discovery">Find people</button>
      <button type="button" data-section="Following">Manage follows</button>
      <button type="button" data-section="Watches">Watch public posts</button>
      <button type="button" data-section="Followers">${pendingFollowers ? `Review ${pendingFollowers} follower${pendingFollowers === 1 ? "" : "s"}` : "Followers"}</button>
      <button type="button" data-section="Moderation">${queuedReplies ? `Review ${queuedReplies} repl${queuedReplies === 1 ? "y" : "ies"}` : "Moderation"}</button>
    </div>
  </article>`;
}

function privacyStatusView(data: OwnerSnapshot) {
  const visibility = data.settings.default_visibility;
  return `<article class="panel privacy-status">
    <div class="section-heading">
      <h2>Sharing defaults</h2>
      <span class="pill ${visibility === "Public" ? "warn" : "ok"}">${escapeHtml(audienceLabel(visibility))}</span>
    </div>
    <dl>
      <dt>Default protocol</dt><dd>${escapeHtml(data.settings.default_protocol)}</dd>
      <dt>Public account</dt><dd>${escapeHtml(data.profile.public_handle)}</dd>
      <dt>Follower list</dt><dd>Owner-only view</dd>
      <dt>Following list</dt><dd>Owner-only view</dd>
    </dl>
  </article>`;
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

function isHomeLane(value: string): value is HomeLane {
  return homeLaneOrder.includes(value as HomeLane);
}

function isDailyQueueFilter(value: string): value is DailyQueueFilter {
  return dailyQueueFilterOrder.includes(value as DailyQueueFilter);
}

function applyFeedPreset(id: string) {
  const preset = feedPresets.find((row) => row.id === id);
  if (!preset) return;
  activeFeedPresetId = preset.id;
  activeHomeLane = preset.lane;
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
  const activeAccount = activeAccountProfile();
  const identityLabel = activeAccount?.label || data.profile.display_name || data.profile.public_handle;
  const identityDetail = activeAccount
    ? `${shortHost(activeAccount.instance_url)} · ${data.profile.public_handle}`
    : `${shortHost(data.settings.instance_url)} · ${data.profile.public_handle}`;
  return `<form id="compose-form" class="panel compose">
    <div class="compose-head">
      <h2>New post</h2>
      <span class="pill ${data.settings.default_visibility === "Public" ? "warn" : "ok"}">${escapeHtml(audienceLabel(data.settings.default_visibility))}</span>
    </div>
    <section class="compose-identity">
      <span class="section-label">Posting as</span>
      <strong>${escapeHtml(identityLabel || "Dais account")}</strong>
      <span>${escapeHtml(identityDetail)}</span>
    </section>
    <fieldset class="audience-picker">
      <legend>Who can see this?</legend>
      <div class="audience-options">
        ${audienceOption("Followers", "Followers", `Approved followers (${approvedFollowers.length})`, "Private default", state.visibility)}
        ${audienceOption("Public", "Public internet", "Open web and public feeds", "Requires explicit public state", state.visibility)}
        ${audienceOption("Unlisted", "Unlisted", "Visible by link", "Still shareable outside Dais", state.visibility)}
        ${audienceOption("Direct", "Direct / E2EE", "Named recipients only", "Use E2EE for encrypted DMs", state.visibility)}
      </div>
      <p class="audience-help">Close friends and custom groups use Audience list. Encrypted DM uses Direct with E2EE enabled.</p>
    </fieldset>
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
      <label>Advanced route
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
      <div class="section-heading">
        <h3>Media</h3>
        <span class="pill ${state.visibility === "Public" ? "warn" : "ok"}">${escapeHtml(mediaAccessLabel(state.visibility))}</span>
      </div>
      <p class="privacy-note media-access-note">${escapeHtml(mediaAccessNote(state.visibility))}</p>
      <label>Alt text / media description
        <input name="media_description" value="${escapeAttr(draftMediaDescription)}" placeholder="Describe the image or video before uploading" />
      </label>
      <div class="media-row">
        <input id="media-file" type="file" accept="image/jpeg,image/png,image/gif,image/webp,video/mp4,video/webm" />
        <button id="media-upload" type="button">Upload</button>
      </div>
      ${list(
        draftAttachments.map((url) => `<article class="attachment-chip">
          <span>${escapeHtml(attachmentLabel(url))}</span>
          <button type="button" data-remove-attachment="${escapeAttr(url)}">Remove</button>
          ${
            attachmentUrlFromDraft(url)
              ? `<button type="button" data-revoke-attachment="${escapeAttr(url)}">Revoke upload</button>`
              : ""
          }
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
      <button type="submit">${escapeHtml(composeSubmitLabel(state))}</button>
    </div>
  </form>`;
}

function mediaAccessLabel(visibility: Visibility) {
  return visibility === "Public" || visibility === "Unlisted" ? "Public media" : "Private media";
}

function mediaAccessNote(visibility: Visibility) {
  if (visibility === "Public") {
    return "Public media uploads use public URLs and can be copied, indexed, or shared outside Dais.";
  }
  if (visibility === "Unlisted") {
    return "Unlisted media uploads use public URLs; anyone with the URL may share them.";
  }
  return "Media uploaded now uses private access for followers-only and direct posts.";
}

function audienceOption(
  value: Visibility,
  label: string,
  detail: string,
  consequence: string,
  selected: Visibility
) {
  const active = value === selected;
  return `<label class="audience-option ${active ? "selected" : ""} ${value === "Public" ? "public" : ""}">
    <input type="radio" name="visibility" value="${escapeAttr(value)}"${active ? " checked" : ""} />
    <strong>${escapeHtml(label)}</strong>
    <span>${escapeHtml(detail)}</span>
    <small>${escapeHtml(consequence)}</small>
  </label>`;
}

function composeSubmitLabel(state: ComposeDraftState) {
  if (state.encrypt && state.visibility === "Direct") return "Send Encrypted DM";
  if (state.visibility === "Direct") return "Send Direct";
  if (state.visibility === "Public") return "Post Publicly";
  if (state.visibility === "Unlisted") return "Post Unlisted";
  return "Post to Followers";
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
  draftMediaDescription = String(form.get("media_description") || "");
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
  const requiresWarningConfirmation = composeWarningOverrideRequired(state, sensitiveCategories, draftAttachments.length > 0);
  const surfaces = visibilitySurfaceRows(state);
  return `<article class="preview-card">
    <div class="section-heading">
      <h3>Audience preview</h3>
      <span class="pill ${state.visibility === "Public" ? "warn" : "ok"}">${escapeHtml(audienceLabel(state.visibility))}</span>
    </div>
    <p>${escapeHtml(audience)}</p>
    <ul class="visibility-facts">
      ${visibilityFacts(state, recipients.length).map((fact) => `<li>${escapeHtml(fact)}</li>`).join("")}
    </ul>
    <div class="surface-preview">
      <span class="section-label">Where this can appear</span>
      <dl>
        ${surfaces.map((row) => `<dt>${escapeHtml(row.label)}</dt><dd>${escapeHtml(row.value)}</dd>`).join("")}
      </dl>
    </div>
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
      requiresWarningConfirmation
        ? `<label class="check warning-confirmation"><input name="confirm_warnings" type="checkbox" /> I reviewed these warnings and still want to publish.</label>`
        : ""
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

function composeWarningOverrideRequired(
  state: ComposeDraftState,
  sensitiveCategories: SensitiveCategory[],
  hasAttachments: boolean
) {
  if (state.visibility === "Public" && hasAttachments) return true;
  if (!sensitiveCategories.length) return false;
  return state.visibility === "Public" || state.visibility === "Unlisted" || state.visibility === "Followers";
}

function visibilityFacts(state: ComposeDraftState, recipientCount: number) {
  if (state.encrypt && state.visibility === "Direct") {
    return [
      `Encrypted DM for ${recipientCount || "no"} named recipient${recipientCount === 1 ? "" : "s"}.`,
      "Dais encrypts the message content before sending it.",
    ];
  }
  if (state.visibility === "Direct") {
    return [
      `Direct post for ${recipientCount || "no"} named recipient${recipientCount === 1 ? "" : "s"}.`,
      "Recipient servers may process plaintext unless E2EE is enabled.",
    ];
  }
  if (state.visibility === "Public") {
    return [
      "Public internet-visible post.",
      "Anyone may copy, index, boost, quote, or archive it.",
    ];
  }
  if (state.visibility === "Unlisted") {
    return [
      "Link-visible post kept out of most public listing surfaces.",
      "Anyone with the URL may still share it.",
    ];
  }
  return [
    "Followers-only post for approved followers.",
    "Approved follower servers receive a delivered copy.",
  ];
}

function visibilitySurfaceRows(state: ComposeDraftState) {
  const routesBluesky = state.protocol === "Both" || state.protocol === "AtProto";
  if (state.visibility === "Public") {
    return [
      { label: "ActivityPub public feeds", value: "Yes" },
      { label: "Bluesky", value: routesBluesky ? "Yes" : "No, route not selected" },
      { label: "Search", value: "Possible" },
      { label: "Boosts/reposts", value: "Allowed by public recipients" },
      { label: "Profile pages", value: "Yes" },
      { label: "Remote inboxes", value: "Delivered to addressed recipients" },
    ];
  }
  if (state.visibility === "Unlisted") {
    return [
      { label: "ActivityPub public feeds", value: "Usually no" },
      { label: "Bluesky", value: routesBluesky ? "Would become public; review route" : "No, route not selected" },
      { label: "Search", value: "Possible by URL or remote indexing" },
      { label: "Boosts/reposts", value: "May spread if recipients share it" },
      { label: "Profile pages", value: "Link-visible where supported" },
      { label: "Remote inboxes", value: "Delivered to addressed recipients" },
    ];
  }
  if (state.visibility === "Direct") {
    return [
      { label: "ActivityPub public feeds", value: "No" },
      { label: "Bluesky", value: "No; direct visibility is not supported" },
      { label: "Search", value: "No public search target" },
      { label: "Boosts/reposts", value: "Not a public repost target" },
      { label: "Profile pages", value: "No" },
      { label: "Remote inboxes", value: state.encrypt ? "Encrypted content delivered" : "Plaintext delivered to recipient servers" },
    ];
  }
  return [
    { label: "ActivityPub public feeds", value: "No" },
    { label: "Bluesky", value: "No; followers-only is not supported" },
    { label: "Search", value: "No public search target" },
    { label: "Boosts/reposts", value: "Limited to recipient behavior" },
    { label: "Profile pages", value: "Hidden from anonymous public view" },
    { label: "Remote inboxes", value: state.encrypt ? "Encrypted content delivered" : "Plaintext delivered to follower servers" },
  ];
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
  if ((state.visibility === "Followers" || state.visibility === "Direct") && !state.encrypt) {
    warnings.push("Private federation is not E2EE; recipient server operators may be able to read delivered copies.");
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
  const activeAccount = activeAccountProfile();
  return `<section class="split">
    <article class="panel settings account-manager">
      <div class="section-heading">
        <h2>Accounts</h2>
        <span class="pill ${accountProfiles.length > 1 ? "ok" : "warn"}">${accountProfiles.length} configured</span>
      </div>
      <p class="privacy-note">Accounts are local client profiles. Switching changes which Dais instance receives reads, posts, replies, follows, watches, moderation, and operator commands.</p>
      ${list(accountProfiles.map(accountCard), "No local account profiles are configured.")}
    </article>
    <form id="settings-form" class="panel settings">
      <div>
        <h2>Add or update account</h2>
        <p>${activeAccount ? `Active: ${escapeHtml(activeAccount.label)} · ${escapeHtml(shortHost(activeAccount.instance_url))}` : "Connect a Dais owner API token."}</p>
      </div>
      <label>Account label
        <input name="label" value="${escapeAttr(activeAccount?.label || "")}" placeholder="Skeptical Engineer" />
      </label>
      <label>Instance URL
        <input name="instance" value="${escapeAttr(activeAccount?.instance_url || data.settings.instance_url)}" placeholder="https://skeptical.engineer" />
      </label>
      <label>Owner token
        <input name="token" type="password" placeholder="${activeAccount?.owner_token_present || data.settings.owner_token_present ? "stored" : "required"}" />
      </label>
      <p>Saving an existing instance updates its local label or token. Saving a new instance creates it and makes it active.</p>
      <button>Save account</button>
    </form>
  </section>`;
}

function accountCard(account: OwnerAccountProfile) {
  return `<article class="item account-item">
    <h3>${escapeHtml(account.label)}</h3>
    <p>${escapeHtml(account.instance_url)}</p>
    <footer>
      <span class="pill ${account.active ? "ok" : ""}">${account.active ? "Active" : "Saved"}</span>
      <span class="pill ${account.owner_token_present ? "ok" : "warn"}">${account.owner_token_present ? "Token stored" : "Token needed"}</span>
      ${account.active ? "" : `<button type="button" data-account-switch="${escapeAttr(account.id)}">Use</button>`}
      ${!account.active && accountProfiles.length > 1 ? `<button type="button" data-account-delete="${escapeAttr(account.id)}">Forget</button>` : ""}
    </footer>
  </article>`;
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
      ${postBodyHtml(post.content)}
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
      <button type="button" data-delete-post="${escapeAttr(post.id)}">Delete post</button>
    </footer>
  </article>`;
}

function postDetailView(post: OwnerPostDetail) {
  return `<div class="detail-body">
    ${postBodyHtml(post.content, post.content_html)}
    <div class="detail-actions">
      <button type="button" data-timeline-action="reply" data-object="${escapeAttr(post.id)}">Reply</button>
      <button type="button" data-copy-link="${escapeAttr(post.id)}">Copy link</button>
      <button type="button" data-open-link="${escapeAttr(post.id)}">Open original</button>
      <button type="button" data-delete-post="${escapeAttr(post.id)}">Delete post</button>
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
    ${postAttachmentsView(post)}
    ${postDetailReplies(post.replies || [])}
    ${postDetailInteractions("Likes", post.likes || [])}
    ${postDetailInteractions("Boosts", post.boosts || [])}
  </div>`;
}

function postAttachmentsView(post: OwnerPostDetail) {
  return `<section class="detail-list">
    <h3>Media</h3>
    ${
      post.attachments?.length
        ? post.attachments.map((attachment) => {
            const url = attachment.url || "";
            return `<article>
              <strong>${escapeHtml(attachment.name || shortUrl(url) || "media")}</strong>
              <span>${escapeHtml(attachment.mediaType || "")}</span>
              ${url ? `<button type="button" data-revoke-media="${escapeAttr(url)}">Revoke media</button>` : ""}
            </article>`;
          }).join("")
        : `<p>None</p>`
    }
  </section>`;
}

function postDetailReplies(rows: PostReply[]) {
  return `<section class="detail-list">
    <h3>Replies</h3>
    ${
      rows.length
        ? rows.map((row) => `<article>
            <strong>${escapeHtml(row.actor_display_name || row.actor_username || actorLabel(row.actor_id))}</strong>
            ${postBodyHtml(row.content || row.id, row.content_html)}
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
      ${postBodyHtml(post.content, post.content_html)}
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

function compactAudienceLabel(value: unknown) {
  const normalized = String(value || "unknown").toLowerCase();
  if (normalized === "public") return "Public";
  if (normalized === "unlisted") return "Unlisted";
  if (normalized === "followers" || normalized === "private") return "Followers";
  if (normalized === "direct") return "Direct";
  return String(value || "Unknown");
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
      ${source.excerpt ? postBodyHtml(source.excerpt) : ""}
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

function watchSubscriptionCard(source: SourceSubscription) {
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(source.title || source.url)}</h2>
      <p>${escapeHtml(source.url)}</p>
    </div>
    <footer>
      <span>${escapeHtml(watchLabel(source.source_type))}</span>
      <span>${escapeHtml(source.status)}</span>
      <span>${source.refresh_cadence_minutes} min</span>
      ${source.last_fetched_at ? `<time>${escapeHtml(formatTime(source.last_fetched_at))}</time>` : ""}
      ${source.last_error ? `<span>${escapeHtml(source.last_error)}</span>` : ""}
    </footer>
    <div class="row-actions">
      <button type="button" data-watch-refresh="${escapeAttr(source.id)}">Refresh</button>
      <button type="button" data-watch-remove="${escapeAttr(source.id)}">Remove</button>
    </div>
  </article>`;
}

function watchLabel(sourceType: string) {
  if (sourceType === "watch_activitypub_actor") return "ActivityPub actor";
  if (sourceType === "watch_activitypub_object") return "ActivityPub post";
  if (sourceType === "watch_bluesky_actor") return "Bluesky actor";
  if (sourceType === "watch_bluesky_post") return "Bluesky post";
  if (sourceType === "watch_rss") return "RSS feed";
  if (sourceType === "watch_atom") return "Atom feed";
  return sourceType;
}

function notificationCard(row: OwnerNotification) {
  const actor = row.actor_display_name || row.actor_username || actorLabel(row.actor_id);
  const read = isRead(row.read);
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.type)} from ${escapeHtml(actor)}</h2>
      ${postBodyHtml(row.content || row.activity_id || "")}
    </div>
    <footer>
      <span>${read ? "read" : "unread"}</span>
      ${row.post_id ? `<span>${escapeHtml(shortUrl(row.post_id))}</span>` : ""}
      ${row.created_at ? `<time>${escapeHtml(formatTime(row.created_at))}</time>` : ""}
      ${read ? "" : `<button type="button" data-notification-read="${escapeAttr(row.id)}">Mark read</button>`}
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
      ${postBodyHtml(row.content)}
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
      ${postBodyHtml(row.content, row.content_html)}
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
  const actions = new Set(row.actions || []);
  const watchButton = actions.has("watch") && row.watch_type && row.watch_target
    ? `<button type="button" data-search-watch="1" data-watch-type="${escapeAttr(row.watch_type)}" data-watch-target="${escapeAttr(row.watch_target)}" data-watch-title="${escapeAttr(row.actor_display_name || row.actor_handle || row.content || row.url)}">Watch</button>`
    : "";
  const replyButton = actions.has("reply") && row.reply_target
    ? `<button type="button" data-search-reply="${escapeAttr(row.reply_target)}">Reply</button>`
    : "";
  const likeButton = actions.has("like")
    ? `<button type="button" data-search-interaction="like" data-object="${escapeAttr(row.id)}">Like</button>`
    : "";
  const boostButton = actions.has("boost")
    ? `<button type="button" data-search-interaction="boost" data-object="${escapeAttr(row.id)}">Boost</button>`
    : "";
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.actor_display_name || row.actor_handle || shortUrl(row.url))}</h2>
      ${postBodyHtml(row.content || row.summary || "", row.content_html)}
      ${sensitivityBadgesHtml([row.actor_display_name || "", row.actor_handle || "", row.content || "", row.summary || ""].join(" "))}
    </div>
    <footer>
      <span>${escapeHtml(row.network)}</span>
      <span>${escapeHtml(row.provider)}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      <a href="${escapeAttr(row.url)}">${escapeHtml(shortHost(row.url))}</a>
      <button type="button" data-open-link="${escapeAttr(row.url)}">Open</button>
      ${watchButton}
      ${replyButton}
      ${likeButton}
      ${boostButton}
    </footer>
  </article>`;
}

function searchPublicActorCard(row: OwnerSearchResult["public_actors"][number]) {
  const actions = new Set(row.actions || []);
  const watchButton = actions.has("watch") && row.watch_type && row.watch_target
    ? `<button type="button" data-search-watch="1" data-watch-type="${escapeAttr(row.watch_type)}" data-watch-target="${escapeAttr(row.watch_target)}" data-watch-title="${escapeAttr(row.display_name || row.handle || row.id)}">Watch</button>`
    : "";
  const followButton = actions.has("follow") && row.follow_target
    ? `<button type="button" data-search-follow="${escapeAttr(row.follow_target)}">Follow</button>`
    : "";
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.display_name || row.handle || actorLabel(row.id))}</h2>
      <p>${escapeHtml(row.handle || row.id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.network)}</span>
      <span>${escapeHtml(row.provider)}</span>
      ${row.url ? `<a href="${escapeAttr(row.url)}">${escapeHtml(shortHost(row.url))}</a>` : ""}
      ${row.url ? `<button type="button" data-open-link="${escapeAttr(row.url)}">Open</button>` : ""}
      ${watchButton}
      ${followButton}
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
      ${row.excerpt ? postBodyHtml(row.excerpt) : ""}
    </div>
    <footer>
      <span>${escapeHtml(row.source_type)}</span>
      <span>${row.read ? "Read" : "Unread"}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      ${row.canonical_url ? `<a href="${escapeAttr(row.canonical_url)}">${escapeHtml(shortHost(row.canonical_url))}</a>` : ""}
    </footer>
  </article>`;
}

function metric(label: string, value: string | number, detail: string, section?: string) {
  const content = `
    <span>${escapeHtml(label)}</span>
    <strong>${escapeHtml(String(value))}</strong>
    ${detail ? `<small>${escapeHtml(detail)}</small>` : ""}`;
  return section
    ? `<button type="button" class="metric-card" data-section="${escapeAttr(section)}">${content}</button>`
    : `<article>${content}</article>`;
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
      ${followerStatusActions(row)}
    </footer>
  </article>`;
}

function followerStatusActions(row: OwnerSnapshot["followers"][number]) {
  const status = String(row.status || "").toLowerCase();
  const follower = escapeAttr(row.follower_actor_id);
  if (status === "pending") {
    return [
      followerStatusButton(follower, "approved", "Approve"),
      followerStatusButton(follower, "rejected", "Reject")
    ].join("");
  }
  if (status === "approved" || status === "accepted") {
    return followerStatusButton(follower, "rejected", "Remove");
  }
  if (status === "rejected" || status === "removed") {
    return followerStatusButton(follower, "approved", "Approve");
  }
  return [
    followerStatusButton(follower, "approved", "Approve"),
    followerStatusButton(follower, "rejected", "Reject")
  ].join("");
}

function followerStatusButton(follower: string, status: string, label: string) {
  return `<button type="button" data-follower-status="${escapeAttr(status)}" data-follower="${follower}">${escapeHtml(label)}</button>`;
}

function followingCard(row: OwnerSnapshot["following"][number]) {
  const canUnfollow = followingStatusCanUnfollow(row.status);
  return `<article class="panel item follower">
    <div>
      <h2>${escapeHtml(actorLabel(row.target_actor_id))}</h2>
      <p>${escapeHtml(row.target_actor_id)}</p>
    </div>
    <footer>
      <span>${escapeHtml(row.status)}</span>
      ${row.accepted_at ? `<time>${escapeHtml(formatTime(row.accepted_at))}</time>` : ""}
      ${canUnfollow ? `<button type="button" data-unfollow="${escapeAttr(row.target_actor_id)}">Unfollow</button>` : ""}
    </footer>
  </article>`;
}

function followingStatusCanUnfollow(status: string) {
  const normalized = String(status || "").toLowerCase();
  return !["removed", "rejected", "unfollowed", "none"].includes(normalized);
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
  const followButton = canFollowDiscoveredStatus(status) ? `<button type="button" data-follow-discovered="1">Follow</button>` : "";
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
      ${followButton}
    </footer>
  </article>`;
}

function canFollowDiscoveredStatus(status: string) {
  const normalized = String(status || "").toLowerCase().replaceAll("_", "-");
  return !["accepted", "following", "requested", "pending"].includes(normalized);
}

function discoveredPostCard(post: DiscoveredPost) {
  const title = post.name || post.content || post.url || post.id;
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(title)}</h2>
      ${post.content && post.content !== title ? postBodyHtml(post.content) : ""}
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
  const actions = moderationReplyActions(row);
  return `<article class="panel item">
    <div>
      <h2>${escapeHtml(row.actor_display_name || row.actor_username || actorLabel(row.actor_id))}</h2>
      ${postBodyHtml(row.content)}
      ${row.moderation_flags?.length ? `<div class="sensitivity-tags">${row.moderation_flags.map((label) => `<span class="sensitive-chip">${escapeHtml(label)}</span>`).join("")}</div>` : ""}
      ${ai?.summary ? `<p class="privacy-note">AI advisory${ai.model ? ` (${escapeHtml(shortModel(ai.model))})` : ""}: ${escapeHtml(ai.summary)}</p>` : ""}
    </div>
    <footer>
      <span>${escapeHtml(row.moderation_status || "approved")}</span>
      ${row.moderation_score != null ? `<span>${escapeHtml(row.moderation_score.toFixed(2))}</span>` : ""}
      <span>${isHiddenValue(row.hidden) ? "hidden" : "visible"}</span>
      ${row.published_at ? `<time>${escapeHtml(formatTime(row.published_at))}</time>` : ""}
      ${actions}
    </footer>
  </article>`;
}

function moderationReplyActions(row: ModerationReply) {
  const status = String(row.moderation_status || "").toLowerCase();
  const hidden = isHiddenValue(row.hidden) || status === "hidden";
  const actions = [];
  if (status !== "approved") actions.push(replyStatusButton(row.id, "approved", "Approve"));
  if (!hidden) actions.push(replyStatusButton(row.id, "hidden", "Hide"));
  if (status !== "rejected") actions.push(replyStatusButton(row.id, "rejected", "Reject"));
  return actions.join("");
}

function replyStatusButton(replyId: string, status: string, label: string) {
  return `<button type="button" data-reply-id="${escapeAttr(replyId)}" data-reply-status="${escapeAttr(status)}">${escapeHtml(label)}</button>`;
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
    label: String(form.get("label") || ""),
    instanceUrl: String(form.get("instance") || ""),
    ownerToken: String(form.get("token") || "")
  });
  resetAccountScopedState();
  notice = "Account saved and activated.";
  await load();
}

async function switchAccount(accountId: string) {
  if (!accountId) return;
  const account = accountProfiles.find((profile) => profile.id === accountId);
  await ownerInvoke("switch_owner_account", { accountId });
  resetAccountScopedState();
  notice = `Switched to ${account?.label || "selected account"}.`;
  await load();
}

async function deleteAccount(accountId: string) {
  if (!accountId) return;
  const account = accountProfiles.find((profile) => profile.id === accountId);
  await ownerInvoke("delete_owner_account", { accountId });
  resetAccountScopedState();
  notice = `Forgot ${account?.label || "account"}.`;
  await load();
}

function resetAccountScopedState() {
  snapshot = null;
  composeState = null;
  draftAttachments = [];
  draftReplyTo = "";
  discoveredActor = null;
  selectedPostDetail = null;
  notifications = [];
  deliveries = [];
  sourceSubscriptions = [];
  sourceItems = [];
  watchSubscriptions = [];
  watchItems = [];
  moderationState = null;
  moderationReplies = [];
  directMessages = [];
  searchResults = emptySearchResults();
  ownerStats = null;
  activeHomeLane = "Following";
  activeFeedPresetId = "following-only";
  selectedHomePostId = smokePostId;
  activeDailyQueueFilter = "All";
  dismissedDailyQueueItems = [];
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
  const currentComposeState = composeState || {
    text,
    visibility: normalizeVisibility(visibility),
    protocol: normalizeProtocol(protocol),
    encrypt: form.get("encrypt") === "on",
    audienceListId,
    recipients: String(form.get("recipients") || ""),
    selectedRecipients: form.getAll("follower_recipient").map((value) => String(value)),
  };
  const sensitiveCategories = sensitiveCategoriesForText(currentComposeState.text);
  const audienceList = snapshot ? selectedAudienceList(snapshot, currentComposeState) : null;
  const disallowedCategories =
    audienceList
      ? sensitiveCategories.filter((label) => !audienceList.allowed_categories.includes(label))
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
  if (
    composeWarningOverrideRequired(currentComposeState, sensitiveCategories, draftAttachments.length > 0) &&
    form.get("confirm_warnings") !== "on"
  ) {
    notice = "Review the compose warnings and check the confirmation before publishing.";
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
  const description = String(data.get("media_description") || "").trim();
  draftMediaDescription = description;
  if (!description) {
    notice = "Add alt text or a media description before uploading.";
    render();
    return;
  }
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
    description,
    access,
    dataBase64
  });
  draftAttachments = [...draftAttachments, JSON.stringify(uploaded.attachment)];
  notice = `Attached ${file.name} as ${uploaded.access || access} media.`;
  render();
}

async function revokeDraftAttachment(value: string) {
  if (!value) return;
  const url = attachmentUrlFromDraft(value);
  if (!url) {
    draftAttachments = draftAttachments.filter((item) => item !== value);
    notice = "Removed local draft attachment.";
    render();
    return;
  }
  await ownerInvoke<{ ok: boolean }>("revoke_owner_media", { url });
  draftAttachments = draftAttachments.filter((item) => item !== value);
  notice = `Revoked upload ${shortUrl(url)}.`;
  render();
}

async function revokeMediaUrl(url: string) {
  if (!url) return;
  await ownerInvoke<{ ok: boolean }>("revoke_owner_media", { url });
  notice = `Revoked media ${shortUrl(url)}.`;
  await load();
  if (selectedPostDetail) {
    selectedPostDetail = await ownerInvoke<OwnerPostDetail>("owner_post_detail", { objectId: selectedPostDetail.id });
    render();
  }
}

async function deletePost(objectId: string) {
  if (!objectId) return;
  const deleted = await ownerInvoke<DeletedPost>("delete_owner_post", { objectId });
  if (selectedPostDetail?.id === objectId) {
    selectedPostDetail = null;
  }
  notice = deleted.deleted ? `Deleted ${shortUrl(deleted.id)}.` : `Delete requested for ${shortUrl(deleted.id)}.`;
  await load();
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

async function addWatch(event: SubmitEvent) {
  event.preventDefault();
  const form = new FormData(event.currentTarget as HTMLFormElement);
  const target = String(form.get("target") || "").trim();
  if (!target) {
    notice = "Enter a watch target.";
    render();
    return;
  }
  const title = String(form.get("title") || "").trim();
  const cadence = Number(form.get("cadence_minutes") || 60);
  const result = await ownerInvoke<{ source: SourceSubscription }>("add_owner_watch", {
    watchType: String(form.get("watch_type") || "rss"),
    target,
    title: title || null,
    cadenceMinutes: Number.isFinite(cadence) ? cadence : 60
  });
  notice = `Watching ${result.source.title || result.source.url}.`;
  await loadLiveSection("Watches");
}

async function addWatchTarget(watchType: string, target: string, title: string) {
  if (!watchType || !target) return;
  const result = await ownerInvoke<{ source: SourceSubscription }>("add_owner_watch", {
    watchType,
    target,
    title: title || null,
    cadenceMinutes: 60
  });
  notice = `Watching ${result.source.title || result.source.url}.`;
  if (active === "Watches") {
    await loadLiveSection("Watches");
  } else {
    render();
  }
}

async function refreshWatch(id: string | null) {
  const result = await ownerInvoke<{ ok: boolean; items: Array<{ id: string; ok: boolean; error?: string | null }> }>(
    "refresh_owner_watch",
    { id }
  );
  const failures = result.items.filter((item) => !item.ok);
  notice = failures.length
    ? `Watch refresh completed with ${failures.length} error${failures.length === 1 ? "" : "s"}.`
    : `Refreshed ${result.items.length} watch${result.items.length === 1 ? "" : "es"}.`;
  await loadLiveSection("Watches");
}

async function removeWatch(id: string) {
  if (!id) return;
  await ownerInvoke("remove_owner_watch", { id });
  notice = `Removed watch ${id}.`;
  await loadLiveSection("Watches");
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

async function blockActorTarget(actorId: string) {
  if (!actorId) return;
  await ownerInvoke("block_owner_actor", { actorId, reason: null });
  notice = `Blocked ${shortUrl(actorId)}.`;
  if (active === "Moderation") {
    await loadLiveSection("Moderation");
  } else {
    render();
  }
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
  if (section === "Home") {
    const [statsResult, notificationsResult, deliveriesResult, directMessagesResult, watchesResult, moderationResult] = await Promise.allSettled([
      ownerInvoke<OwnerStats>("owner_stats"),
      ownerInvoke<OwnerNotification[]>("owner_notifications"),
      ownerInvoke<OwnerDelivery[]>("owner_deliveries"),
      ownerInvoke<OwnerDirectMessage[]>("owner_direct_messages"),
      ownerInvoke<OwnerSources>("owner_watches"),
      ownerInvoke<ModerationState>("owner_moderation")
    ]);
    if (statsResult.status === "fulfilled") ownerStats = statsResult.value;
    if (notificationsResult.status === "fulfilled") notifications = notificationsResult.value;
    if (deliveriesResult.status === "fulfilled") deliveries = deliveriesResult.value;
    if (directMessagesResult.status === "fulfilled") directMessages = directMessagesResult.value;
    if (watchesResult.status === "fulfilled") {
      watchSubscriptions = watchesResult.value.subscriptions;
      watchItems = watchesResult.value.items;
    }
    if (moderationResult.status === "fulfilled") moderationState = moderationResult.value;
    render();
  } else if (section === "Notifications") {
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
  } else if (section === "Watches") {
    const watches = await ownerInvoke<OwnerSources>("owner_watches");
    watchSubscriptions = watches.subscriptions;
    watchItems = watches.items;
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
  const data = new FormData(form);
  const query = String(data.get("query") || "").trim();
  const scope = String(data.get("scope") || "local").trim() || "local";
  searchProvider = String(data.get("provider") || "all").trim() || "all";
  searchResultType = String(data.get("result_type") || "all").trim() || "all";
  searchServers = String(data.get("servers") || "").trim();
  searchSort = String(data.get("sort") || "latest").trim() || "latest";
  searchAuthor = String(data.get("author") || "").trim();
  searchTag = String(data.get("tag") || "").trim();
  searchLang = String(data.get("lang") || "").trim();
  await executeSearch(query, scope, false);
}

async function executeSearch(query: string, scope: string, confirmPublicSensitive: boolean) {
  searchQuery = query;
  searchScope = scope;
  const servers = searchServers.split(",").map((value) => value.trim()).filter(Boolean);
  const tags = searchTag.split(",").map((value) => value.trim().replace(/^#/, "")).filter(Boolean);
  searchResults = query
    ? normalizeSearchResults(await ownerInvoke<OwnerSearchResult>("owner_search", {
        query,
        scope,
        provider: searchProvider,
        resultType: searchResultType,
        servers,
        sort: searchSort,
        author: searchAuthor || null,
        lang: searchLang || null,
        tags,
        confirmPublicSensitive
      }))
    : emptySearchResults();
  notice = query
    ? searchNotice(searchResults)
    : "";
  render();
}

function searchNotice(results: OwnerSearchResult) {
  const guard = results.public_search_guard;
  const base = `Search returned ${results.posts.length} posts, ${results.users.length} actors, ${(results.sources || []).length} sources, ${(results.source_items || []).length} source items, ${(results.public_posts || []).length} public posts, and ${(results.public_actors || []).length} public actors.`;
  return guard?.blocked ? `${base} Public provider search needs confirmation.` : base;
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
    provider_errors: results.provider_errors || [],
    public_search_guard: {
      ...emptySearchResults().public_search_guard,
      ...(results.public_search_guard || {}),
      categories: results.public_search_guard?.categories || []
    }
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
  if (section === "Home") return "Read, reply, publish, and move through the daily owner queue";
  if (section === "Compose") return "Private-by-default publishing with explicit public and E2EE modes";
  if (section === "Search") return "Find public posts and accounts without exposing local searches";
  if (section === "Discovery") return "Resolve people and public posts before following or watching";
  if (section === "Following") return "Read posts and manage the private list of accounts you follow";
  if (section === "Followers") return "Review requests and keep the public follower graph hidden from casual view";
  if (section === "Audience") return "Direct-audience lists with explicit sensitive-category boundaries";
  if (section === "Friends") return "Owner-only mutual relationships and friend feed";
  if (section === "Posts") return "Inspect, reply to, delete, and revoke media from your posts";
  if (section === "Sources") return "Private reader items from public standards-based sources";
  if (section === "Watches") return "Private public-post monitoring without follows, approvals, or remote subscription records";
  if (section === "Moderation") return "Reply review, federation safety, and AI advisory controls";
  if (section === "Deliveries") return "Outbound delivery queue and federation send status";
  if (section === "Stats") return "Operational counts for posts, relationships, delivery, and moderation";
  if (section === "Settings") return "Connect this client to a Dais owner API token";
  if (section === "Profile") return "Edit the public ActivityPub account profile";
  if (section === "Diagnostics") return "Instance, federation, delivery, and client health";
  return "Owner workspace for the live dais instance";
}

function sectionGroupLabel(section: string) {
  return sectionGroups.find((group) => group.sections.includes(section))?.label || "Owner";
}

function navGlyph(section: string) {
  const glyphs: Record<string, string> = {
    Home: "H",
    Compose: "+",
    Search: "S",
    Discovery: "F",
    Notifications: "N",
    DMs: "D",
    Following: "F",
    Friends: "M",
    Followers: "R",
    Audience: "A",
    Posts: "P",
    Sources: "L",
    Watches: "W",
    Moderation: "!",
    Deliveries: "Q",
    Stats: "#",
    Profile: "U",
    Settings: "*",
    Diagnostics: "?"
  };
  return glyphs[section] || section.slice(0, 1);
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

function attachmentUrlFromDraft(value: string) {
  try {
    const attachment = JSON.parse(value);
    return typeof attachment.url === "string" ? attachment.url : "";
  } catch {
    return "";
  }
}

function attachmentLabel(value: string) {
  const url = attachmentUrlFromDraft(value);
  return url ? shortUrl(url) : shortUrl(value);
}

function stripTags(value: string) {
  return value.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim();
}

const safePostHtmlTags = new Set([
  "a",
  "p",
  "br",
  "strong",
  "b",
  "em",
  "i",
  "code",
  "pre",
  "blockquote",
  "ul",
  "ol",
  "li",
  "span",
  "del",
  "s",
  "sub",
  "sup"
]);

function postBodyHtml(text: string, html?: string | null) {
  const body = html ? sanitizePostHtml(html) : linkifyPlainText(text);
  return `<div class="post-body">${body || ""}</div>`;
}

function sanitizePostHtml(html: string) {
  if (typeof DOMParser === "undefined") {
    return linkifyPlainText(stripTags(html));
  }
  const document = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  sanitizePostChildren(document.body);
  return document.body.innerHTML.trim() || linkifyPlainText(stripTags(html));
}

function sanitizePostChildren(parent: Node) {
  for (const child of Array.from(parent.childNodes)) {
    if (child.nodeType === Node.COMMENT_NODE) {
      child.remove();
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const element = child as HTMLElement;
    const tag = element.tagName.toLowerCase();
    if (tag === "script" || tag === "style" || tag === "iframe" || tag === "object" || tag === "embed") {
      element.remove();
      continue;
    }
    sanitizePostChildren(element);
    if (!safePostHtmlTags.has(tag)) {
      element.replaceWith(...Array.from(element.childNodes));
      continue;
    }
    sanitizePostAttributes(element, tag);
  }
}

function sanitizePostAttributes(element: HTMLElement, tag: string) {
  const href = tag === "a" ? element.getAttribute("href") || "" : "";
  const label = tag === "a" ? element.textContent || "" : "";
  for (const attr of Array.from(element.attributes)) {
    element.removeAttribute(attr.name);
  }
  if (tag !== "a") return;
  const safeHref = safePostHref(href);
  if (!safeHref) {
    element.replaceWith(...Array.from(element.childNodes));
    return;
  }
  if (!label.trim() || looksLikeUrlText(label)) {
    element.textContent = postLinkLabel(safeHref);
  }
  element.setAttribute("href", safeHref);
  element.setAttribute("rel", "nofollow noopener noreferrer");
  element.setAttribute("target", "_blank");
  element.setAttribute("title", safeHref);
}

function safePostHref(value: string) {
  try {
    const url = new URL(value, window.location.href);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.href : "";
  } catch {
    return "";
  }
}

function linkifyPlainText(value: string) {
  const raw = value || "";
  const markdownLinkPattern = /\[([^\]\n]{1,180})\]\((https?:\/\/[^)\s]+|mailto:[^)]+)\)/gi;
  let cursor = 0;
  const parts: string[] = [];
  for (const match of raw.matchAll(markdownLinkPattern)) {
    const index = match.index || 0;
    const label = match[1] || "";
    const url = match[2] || "";
    parts.push(linkifyPlainTextSegment(raw.slice(cursor, index)));
    const href = safePostHref(url);
    parts.push(href ? postLinkAnchor(href, markdownPostLinkLabel(label, href)) : escapeHtml(match[0] || ""));
    cursor = index + (match[0] || "").length;
  }
  parts.push(linkifyPlainTextSegment(raw.slice(cursor)));
  return parts.join("");
}

function linkifyPlainTextSegment(value: string) {
  const escaped = escapeHtml(value || "");
  return escaped
    .replace(/https?:\/\/[^\s<]+/g, (url) => {
      const trailing = url.match(/[),.;!?]+$/)?.[0] || "";
      const clean = trailing ? url.slice(0, -trailing.length) : url;
      const href = safePostHref(clean);
      return href ? `${postLinkAnchor(href, postLinkLabel(clean))}${escapeHtml(trailing)}` : url;
    })
    .replaceAll("\n", "<br>");
}

function postLinkAnchor(href: string, label: string) {
  return `<a href="${escapeAttr(href)}" rel="nofollow noopener noreferrer" target="_blank" title="${escapeAttr(href)}">${escapeHtml(label)}</a>`;
}

function markdownPostLinkLabel(label: string, href: string) {
  const trimmed = label.trim();
  if (!trimmed || looksLikeUrlText(trimmed)) return postLinkLabel(href);
  return trimmed.length > 72 ? `${trimmed.slice(0, 69)}...` : trimmed;
}

function looksLikeUrlText(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return false;
  if (/^https?:\/\//i.test(trimmed) || /^mailto:/i.test(trimmed)) return true;
  return trimmed.length > 48 && /[./?=&]/.test(trimmed);
}

function postLinkLabel(value: string) {
  try {
    const url = new URL(value, window.location.href);
    if (url.protocol === "mailto:") return `Email ${decodeURIComponent(url.pathname || value)}`;
    const host = url.hostname.replace(/^www\./, "");
    return url.pathname === "/" && !url.search ? host : `Read at ${host}`;
  } catch {
    return value.length > 52 ? `${value.slice(0, 24)}...${value.slice(-20)}` : value;
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

function formatFeedTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  }
  return date.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
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
