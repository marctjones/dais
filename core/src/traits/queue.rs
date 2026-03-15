/// Queue abstraction trait for platform-agnostic background jobs
///
/// Implementations:
/// - Cloudflare: Cloudflare Queues
/// - Vercel: QStash (Upstash)
/// - Netlify: Background Functions
/// - Railway: BullMQ (Redis-backed)

use super::PlatformResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait(?Send)]
pub trait QueueProvider {
    /// Send a message to the queue (JSON string)
    ///
    /// # Example
    /// ```rust,ignore
    /// let message = serde_json::to_string(&DeliveryMessage {
    ///     inbox_url: "https://mastodon.social/inbox".into(),
    ///     activity: activity_json,
    /// })?;
    /// queue.send(&message).await?;
    /// ```
    async fn send(&self, message: &str) -> PlatformResult<()>;

    /// Send multiple messages in a batch (JSON strings)
    ///
    /// More efficient than sending individually for bulk operations
    async fn send_batch(&self, messages: Vec<String>) -> PlatformResult<()>;

    /// Schedule a message for future delivery (JSON string)
    ///
    /// # Arguments
    /// * `message` - Message to send (JSON)
    /// * `delay_seconds` - Delay before delivery
    async fn send_delayed(
        &self,
        message: &str,
        delay_seconds: u32,
    ) -> PlatformResult<()>;

    /// Get approximate queue depth
    ///
    /// Returns number of messages waiting to be processed
    async fn depth(&self) -> PlatformResult<u64>;
}

/// Handler for processing queue messages
///
/// Platform workers implement this to process messages from the queue
#[async_trait(?Send)]
pub trait QueueHandler {
    /// Process a single message
    ///
    /// If this returns an error, the message may be retried depending on
    /// the platform's retry policy.
    async fn handle(&self, message: QueueMessage) -> PlatformResult<()>;

    /// Called when a message fails after all retries
    async fn on_failed(&self, message: QueueMessage, error: String) -> PlatformResult<()> {
        // Default: log error (platforms can override for DLQ, etc.)
        eprintln!("Message failed after retries: {:?}, error: {}", message, error);
        Ok(())
    }
}

/// Queue message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Message ID (for deduplication)
    pub id: String,

    /// Message body (JSON string)
    pub body: String,

    /// Timestamp when message was enqueued (RFC3339)
    pub timestamp: String,

    /// Number of delivery attempts
    pub attempts: u32,

    /// Custom metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl QueueMessage {
    /// Deserialize the message body into a typed struct
    pub fn deserialize<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

/// Common message types for dais

/// ActivityPub delivery message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryMessage {
    /// Target inbox URL
    pub inbox_url: String,

    /// ActivityPub activity (JSON)
    pub activity: String,

    /// Actor sending the activity
    pub actor: String,

    /// Shared inbox optimization (if available)
    pub shared_inbox: Option<String>,
}

/// AT Protocol sync message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// PDS URL
    pub pds_url: String,

    /// Commit CID
    pub commit_cid: String,

    /// Repo DID
    pub repo_did: String,
}

/// Media processing message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProcessingMessage {
    /// Media ID
    pub media_id: String,

    /// Original file key in storage
    pub original_key: String,

    /// Processing tasks (resize, transcode, etc.)
    pub tasks: Vec<MediaTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaTask {
    /// Generate thumbnail
    Thumbnail { width: u32, height: u32 },

    /// Resize image
    Resize { width: u32, height: u32 },

    /// Transcode video
    Transcode { codec: String, bitrate: u32 },

    /// Extract video thumbnail
    VideoThumbnail { timestamp_seconds: u32 },
}
