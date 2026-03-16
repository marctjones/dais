/// dais-core: Platform-agnostic ActivityPub/AT Protocol implementation
///
/// This library provides the core social protocol logic as a WASM module
/// that can run on any platform (Cloudflare Workers, Vercel, Netlify, etc.)
///
/// Platform-specific code (database, storage, queues, HTTP) is abstracted
/// behind traits in the `traits` module.

pub mod traits;
pub mod activitypub;
pub mod atproto;
pub mod webfinger;
pub mod sql;
pub mod migrations;
mod error;
mod utils;

pub use error::{CoreError, CoreResult};
pub use traits::{
    DatabaseProvider, StorageProvider, QueueProvider, HttpProvider,
    PlatformError, PlatformResult, Row, Statement,
};

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

/// Main entry point for the dais core library
///
/// This struct holds references to platform providers and exposes
/// methods for ActivityPub and AT Protocol operations.
///
/// Note: This struct is NOT directly exposed to WASM. Platform-specific
/// code creates instances and stores them internally.
pub struct DaisCore {
    pub db: Box<dyn DatabaseProvider>,
    pub storage: Box<dyn StorageProvider>,
    pub queue: Box<dyn QueueProvider>,
    pub http: Box<dyn HttpProvider>,
    pub config: CoreConfig,
}

/// Configuration for dais core
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    /// ActivityPub domain (e.g., "social.dais.social")
    pub activitypub_domain: String,

    /// AT Protocol PDS domain (e.g., "pds.dais.social")
    pub pds_domain: String,

    /// Username (e.g., "social")
    pub username: String,

    /// Private key for signatures (PEM format)
    pub private_key: String,

    /// Public key for actor (PEM format)
    pub public_key: String,

    /// Base URL for media (e.g., "https://media.dais.social")
    pub media_url: String,
}

impl DaisCore {
    /// Create a new DaisCore instance
    pub fn new(
        db: Box<dyn DatabaseProvider>,
        storage: Box<dyn StorageProvider>,
        queue: Box<dyn QueueProvider>,
        http: Box<dyn HttpProvider>,
        config: CoreConfig,
    ) -> Self {
        Self {
            db,
            storage,
            queue,
            http,
            config,
        }
    }

    /// Initialize the database schema
    pub async fn initialize_database(&self) -> CoreResult<()> {
        // TODO: Implement database migrations
        Ok(())
    }

    /// Get server configuration
    pub fn get_config(&self) -> &CoreConfig {
        &self.config
    }

    // ActivityPub methods (to be implemented)

    /// Handle incoming ActivityPub activity to inbox
    pub async fn handle_inbox(
        &self,
        activity_json: String,
        our_actor_url: String,
        moderator: Option<&dyn activitypub::ContentModerator>,
    ) -> CoreResult<()> {
        // Parse the activity
        let activity: activitypub::Activity = serde_json::from_str(&activity_json)?;

        // Process the activity
        activitypub::process_inbox_activity(
            &*self.db,
            &*self.http,
            activity,
            &our_actor_url,
            moderator,
        ).await
    }

    /// Create a new post
    pub async fn create_post(&self, content: String, visibility: String) -> CoreResult<String> {
        // TODO: Implement in activitypub module
        Err(CoreError::Internal("Not implemented".to_string()))
    }

    /// Get actor profile
    pub async fn get_actor(&self, username: String) -> CoreResult<activitypub::Person> {
        activitypub::get_actor(&*self.db, &username, &self.config.activitypub_domain).await
    }

    /// Get actor counts (posts, followers, following)
    pub async fn get_actor_counts(&self, actor_id: String) -> CoreResult<activitypub::ActorCounts> {
        activitypub::get_actor_counts(&*self.db, &actor_id).await
    }

    /// Get followers collection for an actor
    pub async fn get_followers(&self, username: String, page: Option<u32>) -> CoreResult<serde_json::Value> {
        activitypub::get_followers(&*self.db, &username, &self.config.activitypub_domain, page).await
    }

    /// Get following collection for an actor
    pub async fn get_following(&self, username: String, page: Option<u32>) -> CoreResult<serde_json::Value> {
        activitypub::get_following(&*self.db, &username, &self.config.activitypub_domain, page).await
    }

    /// Get outbox posts for an actor
    pub async fn get_outbox_posts(&self, username: String) -> CoreResult<Vec<activitypub::Post>> {
        activitypub::get_outbox_posts(&*self.db, &username).await
    }

    /// Get a single post
    pub async fn get_post(&self, username: String, post_id: String) -> CoreResult<activitypub::Post> {
        activitypub::get_post(&*self.db, &username, &post_id).await
    }

    /// Get post interactions (replies, likes, boosts)
    pub async fn get_post_interactions(&self, post_id: String) -> CoreResult<activitypub::PostInteractions> {
        activitypub::get_post_interactions(&*self.db, &post_id).await
    }

    /// Deliver activity to a remote inbox
    pub async fn deliver_to_inbox(
        &self,
        inbox_url: String,
        actor_url: String,
        activity_json: String,
    ) -> CoreResult<()> {
        activitypub::deliver_to_inbox(
            &*self.http,
            &inbox_url,
            &actor_url,
            &activity_json,
            &self.config.private_key,
        ).await
    }

    /// Create delivery jobs for all followers
    pub async fn create_follower_deliveries(
        &self,
        post_id: String,
        actor_id: String,
        activity_json: String,
    ) -> CoreResult<Vec<String>> {
        activitypub::create_follower_deliveries(&*self.db, &post_id, &actor_id, &activity_json).await
    }

    /// WebFinger lookup
    pub async fn webfinger(&self, resource: String) -> CoreResult<webfinger::WebFingerResponse> {
        webfinger::handle_webfinger(
            &*self.db,
            &resource,
            &self.config.activitypub_domain,
            &self.config.activitypub_domain,
        )
        .await
    }

    // AT Protocol methods (to be implemented)

    /// Handle AT Protocol commit
    pub async fn handle_commit(&self, did: String, commit_cid: String) -> CoreResult<()> {
        // TODO: Implement in atproto module
        Err(CoreError::Internal("Not implemented".to_string()))
    }

    /// Subscribe to repo changes
    pub async fn subscribe_repos(&self) -> CoreResult<()> {
        // TODO: Implement in atproto module
        Err(CoreError::Internal("Not implemented".to_string()))
    }
}

// Non-WASM exports for Rust-to-Rust usage
impl DaisCore {
    /// Get database provider reference
    pub fn db(&self) -> &dyn DatabaseProvider {
        &*self.db
    }

    /// Get storage provider reference
    pub fn storage(&self) -> &dyn StorageProvider {
        &*self.storage
    }

    /// Get queue provider reference
    pub fn queue(&self) -> &dyn QueueProvider {
        &*self.queue
    }

    /// Get HTTP provider reference
    pub fn http(&self) -> &dyn HttpProvider {
        &*self.http
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_config_serialization() {
        let config = CoreConfig {
            activitypub_domain: "social.example.com".to_string(),
            pds_domain: "pds.example.com".to_string(),
            username: "user".to_string(),
            private_key: "PRIVATE_KEY".to_string(),
            public_key: "PUBLIC_KEY".to_string(),
            media_url: "https://media.example.com".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CoreConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.activitypub_domain, deserialized.activitypub_domain);
        assert_eq!(config.username, deserialized.username);
    }
}
