use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

use crate::routing::{Protocol, Visibility};

#[derive(Parser)]
#[command(name = "dais")]
#[command(about = "Operator client for a single-user dais instance")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage local ActivityPub actor mode.
    #[command(subcommand)]
    Actors(ActorsCommand),
    /// Bluesky / AT Protocol operations.
    #[command(subcommand)]
    Bluesky(BlueskyCommand),
    /// Create and publish posts through the capability-aware router.
    #[command(subcommand)]
    Post(TopLevelPostCommand),
    /// Search local or production dais data.
    #[command(subcommand)]
    Search(SearchCommand),
    /// Show server statistics from D1.
    Stats(StatsArgs),
    /// Read timelines.
    #[command(subcommand)]
    Timeline(TopLevelTimelineCommand),
    /// Manage private-mode friend relationships.
    #[command(subcommand)]
    Friends(FriendsCommand),
    /// Use the secure owner API for live instance reader and account workflows.
    #[command(subcommand)]
    Owner(OwnerCommand),
    /// Inspect local ActivityPub followers.
    #[command(subcommand)]
    Followers(FollowersCommand),
    /// Inspect ActivityPub notifications.
    #[command(subcommand)]
    Notifications(NotificationsCommand),
    /// Inspect and process ActivityPub delivery jobs.
    #[command(subcommand)]
    Deliveries(DeliveriesCommand),
    /// End-to-end encryption helpers for dais encryptedMessage v1.
    #[command(subcommand)]
    E2ee(E2eeCommand),
    /// Create and coordinate ActivityPub Event objects.
    #[command(subcommand)]
    Events(EventsCommand),
    /// Upload and attach media for posts.
    #[command(subcommand)]
    Media(MediaCommand),
    /// Manage moderation and federation safety settings.
    #[command(subcommand)]
    Moderation(ModerationCommand),
    /// Expanded analytics and operational reports.
    #[command(subcommand)]
    Reports(ReportsCommand),
    /// Manage public source subscriptions and private reader items.
    #[command(subcommand)]
    Sources(SourcesCommand),
    /// Run instance diagnostics and conformance smoke checks.
    Doctor(DoctorArgs),
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
    },
    /// Launch the Rust terminal UI.
    Tui(TuiArgs),
}

#[derive(Subcommand)]
pub enum BlueskyCommand {
    /// Save Bluesky credentials after validating an ATProto session.
    Login(LoginArgs),
    /// Remove saved Bluesky credentials.
    Logout,
    /// Show the configured Bluesky profile.
    Whoami,
    /// Show a Bluesky profile.
    Profile { handle: String },
    /// Update the configured Bluesky/AT Protocol profile record.
    UpdateProfile(UpdateBlueskyProfileArgs),
    /// Manage Bluesky posts.
    #[command(subcommand)]
    Post(PostCommand),
    /// Read Bluesky timelines.
    #[command(subcommand)]
    Timeline(TimelineCommand),
    /// Manage Bluesky follows.
    #[command(subcommand)]
    Follow(FollowCommand),
}

#[derive(Args)]
pub struct LoginArgs {
    /// Bluesky handle, for example alice.bsky.social.
    pub handle: String,
    /// Password or app password. If omitted, stdin is prompted.
    #[arg(long, env = "DAIS_BLUESKY_PASSWORD")]
    pub password: Option<String>,
    /// ATProto PDS service URL used for session and record writes.
    #[arg(long, default_value = "https://bsky.social")]
    pub service: String,
    /// AppView URL used for profile and timeline reads.
    #[arg(long, default_value = "https://api.bsky.app")]
    pub appview: String,
}

#[derive(Args)]
pub struct UpdateBlueskyProfileArgs {
    #[arg(long)]
    pub display_name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Subcommand)]
pub enum PostCommand {
    /// Create a public Bluesky post.
    Create { text: String },
    /// Reply to a Bluesky post. Requires the parent/root URI and CID.
    Reply {
        text: String,
        #[arg(long)]
        uri: String,
        #[arg(long)]
        cid: String,
        /// Root URI when replying inside an existing thread. Defaults to --uri.
        #[arg(long)]
        root_uri: Option<String>,
        /// Root CID when replying inside an existing thread. Defaults to --cid.
        #[arg(long)]
        root_cid: Option<String>,
    },
    /// Like a Bluesky post.
    Like {
        #[arg(long)]
        uri: String,
        #[arg(long)]
        cid: String,
    },
    /// Remove your like from a Bluesky post.
    Unlike {
        #[arg(long)]
        uri: String,
    },
    /// Repost a Bluesky post.
    Repost {
        #[arg(long)]
        uri: String,
        #[arg(long)]
        cid: String,
    },
    /// Remove your repost of a Bluesky post.
    Unrepost {
        #[arg(long)]
        uri: String,
    },
    /// List posts from yourself or another handle.
    List {
        #[arg(long)]
        handle: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u16,
    },
}

#[derive(Subcommand)]
pub enum TimelineCommand {
    /// Read the authenticated Bluesky home timeline.
    Home {
        #[arg(long, default_value_t = 30)]
        limit: u16,
    },
}

#[derive(Subcommand)]
pub enum FollowCommand {
    /// Follow a Bluesky account.
    Add { handle: String },
    /// Unfollow a Bluesky account.
    Remove { handle: String },
    /// List follows, or followers with --followers.
    List {
        #[arg(long)]
        followers: bool,
        #[arg(long, default_value_t = 50)]
        limit: u16,
    },
}

#[derive(Subcommand)]
pub enum TopLevelPostCommand {
    /// Create a post. Only public posts may route to Bluesky.
    Create(CreatePostArgs),
    /// List recent posts from D1.
    List(ListPostsArgs),
    /// Update a local ActivityPub post and queue an Update for followers.
    Update(UpdatePostArgs),
    /// Delete a local ActivityPub post and queue a Delete for followers.
    Delete(ActivityPubObjectArgs),
    /// Like a remote ActivityPub object.
    Like(ActivityPubObjectArgs),
    /// Undo a local Like activity for a remote ActivityPub object.
    Unlike(ActivityPubObjectArgs),
    /// Boost/reblog a remote ActivityPub object.
    Boost(ActivityPubObjectArgs),
    /// Undo a local Announce activity for a remote ActivityPub object.
    Unboost(ActivityPubObjectArgs),
}

#[derive(Subcommand)]
pub enum ActorsCommand {
    /// Show the configured local actor.
    Show {
        #[arg(long)]
        remote: bool,
        #[arg(long, default_value = "social")]
        username: String,
    },
    /// Set the local actor ActivityStreams type.
    SetType {
        actor_type: ActorType,
        #[arg(long)]
        remote: bool,
        #[arg(long, default_value = "social")]
        username: String,
    },
    /// Update local actor profile metadata.
    Update(UpdateActorArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ActorType {
    Person,
    Group,
    Organization,
}

impl std::fmt::Display for ActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorType::Person => f.write_str("Person"),
            ActorType::Group => f.write_str("Group"),
            ActorType::Organization => f.write_str("Organization"),
        }
    }
}

#[derive(Args)]
pub struct CreatePostArgs {
    pub text: String,
    #[arg(long, value_enum, default_value_t = Visibility::Followers)]
    pub visibility: Visibility,
    /// Shortcut for `--visibility public`.
    #[arg(long)]
    pub public: bool,
    #[arg(long, value_enum, default_value_t = Protocol::Both)]
    pub protocol: Protocol,
    /// End-to-end encrypt the ActivityPub post.
    #[arg(long, alias = "e2ee")]
    pub encrypt: bool,
    /// Encrypted fallback behavior for Mastodon/non-dais recipients.
    #[arg(long, value_enum, default_value_t = E2eeFallbackMode::Strict)]
    pub e2ee_fallback: E2eeFallbackMode,
    /// Recipient in key_id=public_key_pem_file form. Repeat for multiple recipients.
    #[arg(long = "recipient")]
    pub recipients: Vec<String>,
    /// ActivityPub object URL this post replies to.
    #[arg(long)]
    pub reply_to: Option<String>,
    /// ActivityStreams object type to publish.
    #[arg(long, value_enum, default_value_t = ActivityObjectType::Note)]
    pub object_type: ActivityObjectType,
    /// ActivityStreams name/title for rich objects such as Article or Document.
    #[arg(long)]
    pub title: Option<String>,
    /// ActivityStreams summary for rich objects.
    #[arg(long)]
    pub summary: Option<String>,
    /// ActivityStreams Event start time, preferably RFC3339.
    #[arg(long)]
    pub starts_at: Option<String>,
    /// ActivityStreams Event end time, preferably RFC3339.
    #[arg(long)]
    pub ends_at: Option<String>,
    /// ActivityStreams Event location label.
    #[arg(long)]
    pub location: Option<String>,
    /// ActivityStreams Question option. Repeat to create a poll.
    #[arg(long = "poll-option")]
    pub poll_options: Vec<String>,
    /// Allow multiple poll answers. Uses ActivityStreams anyOf instead of oneOf.
    #[arg(long)]
    pub poll_multiple: bool,
    /// ActivityStreams attachment URL. Repeat for multiple attachments.
    #[arg(long = "attachment")]
    pub attachments: Vec<String>,
    /// Direct ActivityPub recipient actor URL. Repeat for multiple recipients.
    #[arg(long = "to")]
    pub to: Vec<String>,
    /// Store/read against production D1 for ActivityPub encrypted posts.
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct UpdateActorArgs {
    #[arg(long)]
    pub remote: bool,
    #[arg(long, default_value = "social")]
    pub username: String,
    /// Local actor URL used in the outgoing ActivityPub Update.
    #[arg(long, default_value = "https://social.dais.social/users/social")]
    pub actor: String,
    #[arg(long)]
    pub display_name: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
    /// ActivityStreams icon URL.
    #[arg(long)]
    pub icon: Option<String>,
    /// ActivityStreams image/header URL.
    #[arg(long)]
    pub image: Option<String>,
}

#[derive(Subcommand)]
pub enum MediaCommand {
    /// Upload a local file to the configured R2 media bucket.
    Upload(UploadMediaArgs),
    /// Create an ActivityStreams attachment JSON object from a media URL.
    Attachment(MediaAttachmentArgs),
}

#[derive(Args)]
pub struct UploadMediaArgs {
    pub path: PathBuf,
    /// R2 object key. Defaults to media/<filename>.
    #[arg(long)]
    pub key: Option<String>,
    /// Public URL base used after upload.
    #[arg(long, default_value = "https://social.dais.social/media")]
    pub public_base_url: String,
    /// R2 bucket name.
    #[arg(long, default_value = "dais-media")]
    pub bucket: String,
    /// Upload to Cloudflare remote R2 instead of local wrangler state.
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct MediaAttachmentArgs {
    pub url: String,
    #[arg(long, default_value = "Document")]
    pub kind: String,
    #[arg(long)]
    pub media_type: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Subcommand)]
pub enum ModerationCommand {
    /// List actor/domain blocks.
    Blocks {
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Block one ActivityPub actor.
    BlockActor {
        actor_id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        remote: bool,
    },
    /// Block one domain.
    BlockDomain {
        domain: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        remote: bool,
    },
    /// Remove an actor or domain block by id/domain.
    Unblock {
        value: String,
        #[arg(long)]
        remote: bool,
    },
    /// Show closed-network and allowlist settings.
    Status {
        #[arg(long)]
        remote: bool,
    },
    /// Enable or disable closed-network filtering.
    ClosedNetwork {
        enabled: bool,
        #[arg(long)]
        remote: bool,
    },
    /// Add or update an allowed federation host.
    Allow {
        host: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        remote: bool,
    },
    /// Remove a federation allowlist host.
    Disallow {
        host: String,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Subcommand)]
pub enum ReportsCommand {
    /// Show the expanded operational summary.
    Summary(StatsArgs),
    /// Show recent activity rows.
    Activity {
        #[arg(long, default_value_t = 20)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Show top posts by locally tracked engagement.
    TopPosts {
        #[arg(long, default_value_t = 20)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Subcommand)]
pub enum SourcesCommand {
    /// Add a standards-based public source subscription.
    Add {
        #[command(subcommand)]
        command: SourceAddCommand,
    },
    /// List configured public source subscriptions.
    List {
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Remove a source subscription and its ingested items.
    Remove {
        id: String,
        #[arg(long)]
        remote: bool,
    },
    /// Refresh one source, or all active sources when no id is supplied.
    Refresh {
        id: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        remote: bool,
    },
    /// List ingested private reader items.
    Items {
        #[arg(long)]
        source_id: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        unread: bool,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Subcommand)]
pub enum SourceAddCommand {
    /// Subscribe to an RSS feed URL.
    Rss(SourceAddArgs),
    /// Subscribe to an Atom feed URL.
    Atom(SourceAddArgs),
    /// Register an official or licensed API source placeholder.
    Api(SourceAddArgs),
}

#[derive(Args, Clone)]
pub struct SourceAddArgs {
    pub url: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub cadence_minutes: Option<u16>,
    #[arg(long)]
    pub api_secret_name: Option<String>,
    #[arg(long, default_value_t = true)]
    pub private_reader_only: bool,
    #[arg(long, default_value_t = true)]
    pub excerpt_only: bool,
    #[arg(long, default_value_t = true)]
    pub link_required: bool,
    #[arg(long, default_value_t = true)]
    pub attribution_required: bool,
    #[arg(long)]
    pub no_image: bool,
    #[arg(long)]
    pub full_text_allowed: bool,
    #[arg(long)]
    pub remote: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ActivityObjectType {
    Note,
    Article,
    Document,
    Event,
    Question,
}

impl std::fmt::Display for ActivityObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityObjectType::Note => f.write_str("Note"),
            ActivityObjectType::Article => f.write_str("Article"),
            ActivityObjectType::Document => f.write_str("Document"),
            ActivityObjectType::Event => f.write_str("Event"),
            ActivityObjectType::Question => f.write_str("Question"),
        }
    }
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Create a private-by-default ActivityPub Event.
    Create(CreateEventArgs),
    /// Invite a remote actor to an Event.
    Invite(EventInviteArgs),
    /// Send an RSVP or participation activity for an Event.
    Rsvp(EventRsvpArgs),
    /// List local Event objects.
    List {
        #[arg(long, default_value_t = 20)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Args)]
pub struct CreateEventArgs {
    pub title: String,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub starts_at: String,
    #[arg(long)]
    pub ends_at: Option<String>,
    #[arg(long)]
    pub location: Option<String>,
    #[arg(long, value_enum, default_value_t = Visibility::Followers)]
    pub visibility: Visibility,
    #[arg(long)]
    pub public: bool,
    #[arg(long = "to")]
    pub to: Vec<String>,
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct EventInviteArgs {
    pub event_id: String,
    pub actor: String,
    #[arg(long)]
    pub inbox: String,
    #[arg(long)]
    pub remote: bool,
    #[arg(long, default_value = "https://social.dais.social/users/social")]
    pub local_actor: String,
}

#[derive(Args)]
pub struct EventRsvpArgs {
    pub event_id: String,
    #[arg(value_enum)]
    pub response: EventRsvp,
    #[arg(long)]
    pub inbox: Option<String>,
    #[arg(long)]
    pub remote: bool,
    #[arg(long, default_value = "https://social.dais.social/users/social")]
    pub actor: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum EventRsvp {
    Accept,
    Reject,
    Join,
    Leave,
}

impl std::fmt::Display for EventRsvp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventRsvp::Accept => f.write_str("Accept"),
            EventRsvp::Reject => f.write_str("Reject"),
            EventRsvp::Join => f.write_str("Join"),
            EventRsvp::Leave => f.write_str("Leave"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum E2eeFallbackMode {
    /// Keyless fallback link. Most secure; key must arrive out of band.
    Strict,
    /// Include the decrypt key in the federated fallback link fragment.
    TrustedServer,
    /// Keep fallback keyless and print a separate decrypt link/key locally.
    SplitChannel,
}

#[derive(Args)]
pub struct TuiArgs {
    /// Read from production D1 instead of local development D1.
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct DoctorArgs {
    /// Social/ActivityPub base URL.
    #[arg(long, default_value = "https://social.dais.social")]
    pub social_base_url: String,
    /// AT Protocol PDS base URL.
    #[arg(long, default_value = "https://pds.dais.social")]
    pub pds_base_url: String,
    /// Local username.
    #[arg(long, default_value = "social")]
    pub username: String,
    /// WebFinger account domain.
    #[arg(long, default_value = "social.dais.social")]
    pub acct_domain: String,
    /// Known public post path or URL used for object dereference smoke checks.
    #[arg(long, default_value = "/users/social/posts/20260608212713-5dafca61")]
    pub public_post: String,
    /// Known private/E2EE post path or URL used for anonymous-denial smoke checks.
    #[arg(long, default_value = "/users/social/posts/20260608215639-2ddf52c8")]
    pub private_post: String,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ListPostsArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: u16,
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct UpdatePostArgs {
    pub post_id: String,
    pub text: String,
    #[arg(long)]
    pub remote: bool,
    /// Local actor URL.
    #[arg(long, default_value = "https://social.dais.social/users/social")]
    pub actor: String,
}

#[derive(Args)]
pub struct ActivityPubObjectArgs {
    pub object_id: String,
    #[arg(long)]
    pub remote: bool,
    /// Local actor URL.
    #[arg(long, default_value = "https://social.dais.social/users/social")]
    pub actor: String,
    /// Target inbox override. If omitted, dais fetches the object actor.
    #[arg(long)]
    pub inbox: Option<String>,
}

#[derive(Subcommand)]
pub enum SearchCommand {
    /// Search posts by content.
    Posts {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Search followers and following by actor URL.
    Users {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Args)]
pub struct StatsArgs {
    #[arg(long)]
    pub remote: bool,
}

#[derive(Subcommand)]
pub enum FriendsCommand {
    /// List mutual approved/accepted follows.
    List {
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        remote: bool,
        /// Local actor URL. Defaults to the production dais actor.
        #[arg(long, default_value = "https://social.dais.social/users/social")]
        actor: String,
    },
}

#[derive(Subcommand)]
pub enum FollowersCommand {
    /// List local ActivityPub followers.
    List {
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Approve a follower and send an ActivityPub Accept.
    Approve {
        follower_actor_id: String,
        #[arg(long)]
        remote: bool,
        /// Local actor URL. Defaults to the production dais actor.
        #[arg(long, default_value = "https://social.dais.social/users/social")]
        actor: String,
        /// Social/ActivityPub base URL that routes /admin/followers/accept.
        #[arg(long, default_value = "https://social.dais.social")]
        base_url: String,
    },
    /// Reject/remove a follower.
    Reject {
        follower_actor_id: String,
        #[arg(long)]
        remote: bool,
        /// Local actor URL. Defaults to the production dais actor.
        #[arg(long, default_value = "https://social.dais.social/users/social")]
        actor: String,
    },
}

#[derive(Subcommand)]
pub enum OwnerCommand {
    /// Show live owner API counts.
    Snapshot(OwnerApiArgs),
    /// Show or update public account profile metadata through the live owner API.
    #[command(subcommand)]
    Profile(OwnerProfileCommand),
    /// Read the live owner API home timeline.
    Timeline(OwnerTimelineArgs),
    /// List actors followed by the live instance.
    Following(OwnerApiArgs),
    /// List mutual friend relationships through the live owner API.
    Friends(OwnerApiArgs),
    /// List live owner API notifications.
    Notifications(OwnerApiArgs),
    /// Mark one live owner API notification as read.
    NotificationRead(OwnerNotificationReadArgs),
    /// List live owner API delivery jobs.
    Deliveries(OwnerApiArgs),
    /// List live owner API direct messages.
    Dms(OwnerApiArgs),
    /// Search live owner API posts and actor relationships.
    Search(OwnerSearchArgs),
    /// Show live owner API server stats.
    Stats(OwnerApiArgs),
    /// Show live owner API diagnostics.
    Diagnostics(OwnerApiArgs),
    /// Create a post through the live owner API.
    PostCreate(OwnerPostCreateArgs),
    /// List live owner API source subscriptions and reader items.
    Sources(OwnerApiArgs),
    /// Upload media through the live owner API and print attachment JSON.
    MediaUpload(OwnerMediaUploadArgs),
    /// Add a source subscription through the live owner API.
    SourceAdd(OwnerSourceAddArgs),
    /// Remove a live owner API source subscription.
    SourceRemove(OwnerSourceIdArgs),
    /// Refresh one live owner API source, or all active sources when no id is supplied.
    SourceRefresh(OwnerSourceRefreshArgs),
    /// Show live owner API moderation blocks and federation allowlist.
    Moderation(OwnerApiArgs),
    /// Block an ActivityPub actor through the live owner API.
    BlockActor(OwnerModerationActorArgs),
    /// Block a domain through the live owner API.
    BlockDomain(OwnerModerationDomainArgs),
    /// Remove an actor or domain block through the live owner API.
    Unblock(OwnerModerationValueArgs),
    /// Allow a federation host through the live owner API.
    AllowHost(OwnerModerationHostArgs),
    /// Remove a federation host from the live owner API allowlist.
    DisallowHost(OwnerModerationHostOnlyArgs),
    /// Resolve an ActivityPub actor before following.
    Discover(OwnerFollowArgs),
    /// Show a live owner API post detail and interaction counts.
    Post(OwnerObjectArgs),
    /// Follow an ActivityPub actor by URL or @user@domain handle.
    Follow(OwnerFollowArgs),
    /// Unfollow an ActivityPub actor by URL or @user@domain handle.
    Unfollow(OwnerFollowArgs),
    /// Like a remote ActivityPub object through the live owner API.
    Like(OwnerObjectArgs),
    /// Undo a live owner API Like.
    Unlike(OwnerObjectArgs),
    /// Boost/reblog a remote ActivityPub object through the live owner API.
    Boost(OwnerObjectArgs),
    /// Undo a live owner API boost.
    Unboost(OwnerObjectArgs),
}

#[derive(Args, Clone, Debug)]
pub struct OwnerApiArgs {
    /// Dais instance base URL.
    #[arg(
        long,
        env = "DAIS_OWNER_INSTANCE_URL",
        default_value = "https://social.dais.social"
    )]
    pub instance_url: String,
    /// Owner API bearer token.
    #[arg(long, env = "DAIS_OWNER_TOKEN")]
    pub owner_token: String,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerSearchArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub query: String,
}

#[derive(Subcommand)]
pub enum OwnerProfileCommand {
    /// Show public profile metadata exposed through ActivityPub and Mastodon APIs.
    Show(OwnerApiArgs),
    /// Update public profile metadata exposed through ActivityPub and Mastodon APIs.
    Update(OwnerProfileUpdateArgs),
}

#[derive(Args, Clone, Debug)]
pub struct OwnerProfileUpdateArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    #[arg(long)]
    pub actor_type: Option<String>,
    #[arg(long)]
    pub display_name: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
    /// ActivityStreams icon/avatar URL.
    #[arg(long)]
    pub icon: Option<String>,
    /// ActivityStreams image/header URL.
    #[arg(long)]
    pub image: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerTimelineArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerPostCreateArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub text: String,
    #[arg(long, value_enum, default_value_t = Visibility::Followers)]
    pub visibility: Visibility,
    /// Shortcut for `--visibility public`.
    #[arg(long)]
    pub public: bool,
    #[arg(long, value_enum, default_value_t = Protocol::ActivityPub)]
    pub protocol: Protocol,
    #[arg(long)]
    pub encrypt: bool,
    /// ActivityPub object URL this post replies to.
    #[arg(long)]
    pub reply_to: Option<String>,
    /// Direct ActivityPub recipient actor URL. Repeat for multiple recipients.
    #[arg(long = "to")]
    pub recipients: Vec<String>,
    /// ActivityStreams attachment URL or JSON object. Repeat for multiple attachments.
    #[arg(long = "attachment")]
    pub attachments: Vec<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerMediaUploadArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub path: PathBuf,
    /// Filename stored in media metadata. Defaults to the local file name.
    #[arg(long)]
    pub filename: Option<String>,
    /// MIME type such as image/png or video/mp4. Guessed from filename when omitted.
    #[arg(long)]
    pub media_type: Option<String>,
    /// Media access mode: public or private.
    #[arg(long)]
    pub access: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerSourceAddArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub source_type: String,
    pub url: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub cadence_minutes: Option<u16>,
    #[arg(long)]
    pub api_secret_name: Option<String>,
    #[arg(long, default_value_t = true)]
    pub private_reader_only: bool,
    #[arg(long, default_value_t = true)]
    pub excerpt_only: bool,
    #[arg(long, default_value_t = true)]
    pub link_required: bool,
    #[arg(long, default_value_t = true)]
    pub attribution_required: bool,
    #[arg(long)]
    pub image_allowed: bool,
    #[arg(long)]
    pub full_text_allowed: bool,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerSourceIdArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub id: String,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerSourceRefreshArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub id: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerModerationActorArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub actor_id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerModerationDomainArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub domain: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerModerationValueArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub value: String,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerModerationHostArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub host: String,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerModerationHostOnlyArgs {
    #[command(flatten)]
    pub api: OwnerApiArgs,
    pub host: String,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerNotificationReadArgs {
    pub id: String,
    #[command(flatten)]
    pub api: OwnerApiArgs,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerFollowArgs {
    pub target: String,
    #[command(flatten)]
    pub api: OwnerApiArgs,
}

#[derive(Args, Clone, Debug)]
pub struct OwnerObjectArgs {
    pub object_id: String,
    #[command(flatten)]
    pub api: OwnerApiArgs,
}

#[derive(Subcommand)]
pub enum NotificationsCommand {
    /// List local ActivityPub notifications.
    List {
        #[arg(long, default_value_t = 50)]
        limit: u16,
        #[arg(long)]
        remote: bool,
    },
    /// Mark one notification as read.
    Read {
        id: String,
        #[arg(long)]
        remote: bool,
    },
}

#[derive(Subcommand)]
pub enum DeliveriesCommand {
    /// List ActivityPub delivery jobs from D1.
    List(ListDeliveriesArgs),
    /// Enqueue one existing queued/retryable delivery for normal worker processing.
    Enqueue(EnqueueDeliveryArgs),
    /// Process one queued or retryable delivery through the deployed delivery worker.
    Process(ProcessDeliveryArgs),
    /// Process queued or retryable deliveries in batch.
    ProcessQueued(ProcessQueuedDeliveriesArgs),
}

#[derive(Args)]
pub struct ListDeliveriesArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: u16,
    /// Filter by status: queued, retry, failed, delivered.
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub remote: bool,
}

#[derive(Args)]
pub struct ProcessDeliveryArgs {
    pub id: String,
    /// Social/ActivityPub base URL that routes /admin/deliveries/process.
    #[arg(long, default_value = "https://social.dais.social")]
    pub base_url: String,
    /// Delivery admin token for the deployed worker.
    #[arg(long, env = "DELIVERY_ADMIN_TOKEN")]
    pub admin_token: Option<String>,
}

#[derive(Args)]
pub struct EnqueueDeliveryArgs {
    pub id: String,
    /// Social/ActivityPub base URL that routes /admin/deliveries/enqueue.
    #[arg(long, default_value = "https://social.dais.social")]
    pub base_url: String,
}

#[derive(Args)]
pub struct ProcessQueuedDeliveriesArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: u16,
    /// Delivery status to process: queued or retry.
    #[arg(long, default_value = "queued")]
    pub status: String,
    /// Social/ActivityPub base URL that routes /admin/deliveries/process.
    #[arg(long, default_value = "https://social.dais.social")]
    pub base_url: String,
    /// Delivery admin token for the deployed worker.
    #[arg(long, env = "DELIVERY_ADMIN_TOKEN")]
    pub admin_token: Option<String>,
    #[arg(long)]
    pub remote: bool,
}

#[derive(Subcommand)]
pub enum E2eeCommand {
    /// Encrypt plaintext and emit a Note payload with fallback content plus encryptedMessage.
    Encrypt(EncryptArgs),
    /// Decrypt an encryptedMessage JSON file or Note payload.
    Decrypt(DecryptArgs),
    /// Render the graceful fallback HTML content.
    Fallback {
        #[arg(long)]
        view_url: Option<String>,
    },
}

#[derive(Args)]
pub struct EncryptArgs {
    pub plaintext: String,
    /// Recipient in key_id=public_key_pem_file form. Repeat for multiple recipients.
    #[arg(long = "recipient", required = true)]
    pub recipients: Vec<String>,
    #[arg(long)]
    pub view_url: Option<String>,
}

#[derive(Args)]
pub struct DecryptArgs {
    /// JSON file containing encryptedMessage or a Note payload with encryptedMessage.
    pub input: String,
    /// PKCS#8 PEM private key file.
    #[arg(long)]
    pub private_key: String,
    /// Recipient key id to select. Optional only when the message has one recipient.
    #[arg(long)]
    pub key_id: Option<String>,
}

#[derive(Subcommand)]
pub enum TopLevelTimelineCommand {
    /// Read the home timeline.
    Home {
        #[arg(long, default_value_t = 30)]
        limit: u16,
        #[arg(long, value_enum, default_value_t = Protocol::Both)]
        protocol: Protocol,
        #[arg(long)]
        remote: bool,
        #[arg(long)]
        before: Option<String>,
    },
}
