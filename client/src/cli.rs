use clap::{Args, Parser, Subcommand};

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
    /// End-to-end encryption helpers for dais encryptedMessage v1.
    #[command(subcommand)]
    E2ee(E2eeCommand),
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
}

#[derive(Args)]
pub struct CreatePostArgs {
    pub text: String,
    #[arg(long, value_enum, default_value_t = Visibility::Followers)]
    pub visibility: Visibility,
    #[arg(long, value_enum, default_value_t = Protocol::Both)]
    pub protocol: Protocol,
}

#[derive(Args)]
pub struct ListPostsArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: u16,
    #[arg(long)]
    pub remote: bool,
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
