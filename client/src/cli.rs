use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

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
    /// Direct ActivityPub recipient actor URL. Repeat for multiple recipients.
    #[arg(long = "to")]
    pub to: Vec<String>,
    /// Store/read against production D1 for ActivityPub encrypted posts.
    #[arg(long)]
    pub remote: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ActivityObjectType {
    Note,
    Article,
    Document,
}

impl std::fmt::Display for ActivityObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityObjectType::Note => f.write_str("Note"),
            ActivityObjectType::Article => f.write_str("Article"),
            ActivityObjectType::Document => f.write_str("Document"),
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
