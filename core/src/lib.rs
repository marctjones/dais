pub mod activitypub;
pub mod atproto;
#[cfg(feature = "mls")]
pub mod e2ee_mls;
pub mod e2ee_wire;
mod error;
pub mod migrations;
pub mod protocol;
pub mod sql;
/// dais-core: shared ActivityPub/AT Protocol implementation
///
/// This library provides the core social protocol logic as a WASM module
/// for Cloudflare Workers.
///
/// Cloudflare-specific code (database, storage, queues, HTTP) is abstracted
/// behind traits in the `traits` module so core logic can be tested directly.
pub mod traits;
mod utils;
pub mod webfinger;

pub use error::{CoreError, CoreResult};
pub use traits::{
    DatabaseProvider, HttpProvider, PlatformError, PlatformResult, QueueProvider, Row, Statement,
    StorageProvider,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        Err(CoreError::Internal(
            "DaisCore::initialize_database is not wired to the real CLI migration set; run the platform migration workflow instead of treating core initialization as complete."
                .to_string(),
        ))
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
            &self.config.private_key,
            moderator,
        )
        .await
    }

    /// Create a new local ActivityPub post.
    ///
    /// Empty visibility uses instance_settings.default_visibility and falls back
    /// to followers-only if settings are unavailable.
    pub async fn create_post(&self, content: String, visibility: String) -> CoreResult<String> {
        let configured_default = self.default_post_visibility().await;
        let visibility = resolve_post_visibility(&visibility, Some(configured_default.as_str()))?;
        let local_post_id = utils::generate_id();
        let post_id = utils::post_url(
            &self.config.activitypub_domain,
            &self.config.username,
            &local_post_id,
        );
        let actor_id = utils::actor_url(&self.config.activitypub_domain, &self.config.username);
        let published_at = utils::now_rfc3339();
        let content_html = utils::sanitize_html(&content);

        let insert_with_protocol = r#"
            INSERT INTO posts (
                id, actor_id, content, content_html, visibility, published_at, protocol
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, 'activitypub'
            )
        "#;

        let params = [
            Value::String(post_id.clone()),
            Value::String(actor_id.clone()),
            Value::String(content.clone()),
            Value::String(content_html.clone()),
            Value::String(visibility.clone()),
            Value::String(published_at.clone()),
        ];

        if let Err(err) = self.db.execute(insert_with_protocol, &params).await {
            let insert_legacy = r#"
                INSERT INTO posts (
                    id, actor_id, content, content_html, visibility, published_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6
                )
            "#;
            self.db
                .execute(insert_legacy, &params)
                .await
                .map_err(|legacy_err| {
                    CoreError::Platform(crate::traits::PlatformError::Database(format!(
                        "post insert failed: {}; legacy insert failed: {}",
                        err, legacy_err
                    )))
                })?;
        }

        Ok(post_id)
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
    pub async fn get_followers(
        &self,
        username: String,
        page: Option<u32>,
    ) -> CoreResult<serde_json::Value> {
        activitypub::get_followers(&*self.db, &username, &self.config.activitypub_domain, page)
            .await
    }

    /// Get following collection for an actor
    pub async fn get_following(
        &self,
        username: String,
        page: Option<u32>,
    ) -> CoreResult<serde_json::Value> {
        activitypub::get_following(&*self.db, &username, &self.config.activitypub_domain, page)
            .await
    }

    /// Get outbox posts for an actor
    pub async fn get_outbox_posts(&self, username: String) -> CoreResult<Vec<activitypub::Post>> {
        activitypub::get_outbox_posts(&*self.db, &username).await
    }

    /// Get a single post
    pub async fn get_post(
        &self,
        username: String,
        post_id: String,
    ) -> CoreResult<activitypub::Post> {
        activitypub::get_post(&*self.db, &username, &post_id).await
    }

    /// Get post interactions (replies, likes, boosts)
    pub async fn get_post_interactions(
        &self,
        post_id: String,
    ) -> CoreResult<activitypub::PostInteractions> {
        activitypub::get_post_interactions(&*self.db, &post_id).await
    }

    /// Get the local home timeline built from signed inbox delivery.
    pub async fn get_home_timeline(
        &self,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Vec<activitypub::TimelinePost>> {
        activitypub::get_home_timeline(&*self.db, limit, before.as_deref()).await
    }

    /// Get friends, derived from approved followers plus accepted following.
    pub async fn get_friends(&self, limit: u32) -> CoreResult<Vec<activitypub::Friend>> {
        let actor_id = utils::actor_url(&self.config.activitypub_domain, &self.config.username);
        activitypub::get_friends(&*self.db, &actor_id, limit).await
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
        )
        .await
    }

    /// Deliver activity to a remote inbox with additional signed headers.
    pub async fn deliver_to_inbox_with_extra_headers(
        &self,
        inbox_url: String,
        actor_url: String,
        activity_json: String,
        extra_headers: Vec<(String, String)>,
    ) -> CoreResult<()> {
        activitypub::deliver_to_inbox_with_extra_headers(
            &*self.http,
            &inbox_url,
            &actor_url,
            &activity_json,
            &self.config.private_key,
            &extra_headers,
        )
        .await
    }

    /// Create delivery jobs for all followers
    pub async fn create_follower_deliveries(
        &self,
        post_id: String,
        actor_id: String,
        activity_json: String,
    ) -> CoreResult<Vec<String>> {
        activitypub::create_follower_deliveries(&*self.db, &post_id, &actor_id, &activity_json)
            .await
    }

    /// WebFinger lookup
    pub async fn webfinger(&self, resource: String) -> CoreResult<webfinger::WebFingerResponse> {
        // Accept the email-style apex handle (@user@domain.com) in addition to the
        // ActivityPub subdomain (@user@social.domain.com). Derive the base domain by
        // stripping one subdomain label (social.dais.social -> dais.social); if the
        // AP domain is already an apex (no extra label), fall back to it unchanged.
        let ap = self.config.activitypub_domain.as_str();
        let base_domain = match ap.split_once('.') {
            Some((_, rest)) if rest.contains('.') => rest,
            _ => ap,
        };
        webfinger::handle_webfinger(&*self.db, &resource, base_domain, ap).await
    }

    /// Handle AT Protocol commit
    pub async fn handle_commit(&self, did: String, commit_cid: String) -> CoreResult<()> {
        let did = did.trim();
        let commit_cid = commit_cid.trim();
        if did.is_empty() || commit_cid.is_empty() {
            return Err(CoreError::InvalidAtProto(
                "handle_commit requires a DID and commit CID".to_string(),
            ));
        }
        let sequence = atproto::sequence_from_stable_value(&format!("{did}:{commit_cid}"));
        let sequence_i64 = i64::try_from(sequence).unwrap_or(i64::MAX);
        let id = format!("{did}:{commit_cid}");
        self.db
            .execute(
                r#"
                INSERT OR IGNORE INTO atproto_sync_commits
                    (id, repo_did, commit_cid, sequence, status, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, 'received', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                "#,
                &[
                    Value::String(id),
                    Value::String(did.to_string()),
                    Value::String(commit_cid.to_string()),
                    Value::Number(sequence_i64.into()),
                ],
            )
            .await?;

        let message = crate::traits::SyncMessage {
            pds_url: format!("https://{}", self.config.pds_domain),
            commit_cid: commit_cid.to_string(),
            repo_did: did.to_string(),
        };
        self.queue.send(&serde_json::to_string(&message)?).await?;

        self.db
            .execute(
                r#"
                UPDATE atproto_sync_commits
                SET status = 'queued', updated_at = CURRENT_TIMESTAMP
                WHERE repo_did = ?1 AND commit_cid = ?2
                "#,
                &[
                    Value::String(did.to_string()),
                    Value::String(commit_cid.to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    /// Subscribe to repo changes
    pub async fn subscribe_repos(&self) -> CoreResult<()> {
        let request = atproto::subscribe_repos_request(&self.atproto_identity());
        self.queue.send(&serde_json::to_string(&request)?).await?;
        Ok(())
    }

    async fn default_post_visibility(&self) -> String {
        let query = "SELECT default_visibility FROM instance_settings WHERE id = 1";
        match self.db.execute(query, &[]).await {
            Ok(rows) => rows
                .first()
                .and_then(|row| row.get("default_visibility"))
                .and_then(|value| value.as_str())
                .filter(|visibility| is_valid_post_visibility(visibility))
                .unwrap_or("followers")
                .to_string(),
            Err(_) => "followers".to_string(),
        }
    }

    fn atproto_identity(&self) -> atproto::AtprotoIdentity {
        atproto::AtprotoIdentity::new(
            format!("did:web:{}", self.config.activitypub_domain),
            self.config.activitypub_domain.clone(),
            self.config.pds_domain.clone(),
        )
    }
}

fn resolve_post_visibility(
    requested: &str,
    configured_default: Option<&str>,
) -> CoreResult<String> {
    let requested = requested.trim();
    if !requested.is_empty() {
        return if is_valid_post_visibility(requested) {
            Ok(requested.to_string())
        } else {
            Err(CoreError::InvalidActivity(format!(
                "Invalid post visibility '{}'",
                requested
            )))
        };
    }

    let configured = configured_default.unwrap_or("followers").trim();
    if is_valid_post_visibility(configured) {
        Ok(configured.to_string())
    } else {
        Ok("followers".to_string())
    }
}

fn is_valid_post_visibility(visibility: &str) -> bool {
    matches!(visibility, "public" | "unlisted" | "followers" | "direct")
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
    use crate::traits::{
        DatabaseDialect, DatabaseProvider, HttpProvider, ListOptions, ListResult, ObjectInfo,
        PlatformError, PlatformResult, QueueProvider, Request, Response, Statement,
        StorageMetadata, StorageProvider,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct RecordingDb {
        calls: Arc<Mutex<Vec<(String, Vec<Value>)>>>,
    }

    #[async_trait(?Send)]
    impl DatabaseProvider for RecordingDb {
        async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<crate::Row>> {
            self.calls
                .lock()
                .unwrap()
                .push((sql.to_string(), params.to_vec()));
            Ok(Vec::new())
        }

        async fn batch(&self, _statements: Vec<Statement>) -> PlatformResult<()> {
            Ok(())
        }

        fn dialect(&self) -> DatabaseDialect {
            DatabaseDialect::SQLite
        }
    }

    #[derive(Clone, Default)]
    struct RecordingQueue {
        messages: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait(?Send)]
    impl QueueProvider for RecordingQueue {
        async fn send(&self, message: &str) -> PlatformResult<()> {
            self.messages.lock().unwrap().push(message.to_string());
            Ok(())
        }

        async fn send_batch(&self, messages: Vec<String>) -> PlatformResult<()> {
            self.messages.lock().unwrap().extend(messages);
            Ok(())
        }

        async fn send_delayed(&self, message: &str, _delay_seconds: u32) -> PlatformResult<()> {
            self.messages.lock().unwrap().push(message.to_string());
            Ok(())
        }

        async fn depth(&self) -> PlatformResult<u64> {
            Ok(self.messages.lock().unwrap().len() as u64)
        }
    }

    #[derive(Default)]
    struct NoopStorage;

    #[async_trait(?Send)]
    impl StorageProvider for NoopStorage {
        async fn put(
            &self,
            _key: &str,
            _data: Vec<u8>,
            _content_type: &str,
        ) -> PlatformResult<String> {
            Err(PlatformError::Storage("not used in core tests".to_string()))
        }

        async fn put_with_metadata(
            &self,
            _key: &str,
            _data: Vec<u8>,
            _content_type: &str,
            _metadata: StorageMetadata,
        ) -> PlatformResult<String> {
            Err(PlatformError::Storage("not used in core tests".to_string()))
        }

        async fn get(&self, _key: &str) -> PlatformResult<Vec<u8>> {
            Err(PlatformError::Storage("not used in core tests".to_string()))
        }

        async fn head(&self, _key: &str) -> PlatformResult<ObjectInfo> {
            Err(PlatformError::Storage("not used in core tests".to_string()))
        }

        async fn delete(&self, _key: &str) -> PlatformResult<()> {
            Ok(())
        }

        async fn list(&self, _prefix: &str) -> PlatformResult<Vec<String>> {
            Ok(Vec::new())
        }

        async fn list_detailed(&self, _options: ListOptions) -> PlatformResult<ListResult> {
            Ok(ListResult {
                objects: Vec::new(),
                cursor: None,
                has_more: false,
            })
        }

        async fn copy(&self, _from: &str, _to: &str) -> PlatformResult<()> {
            Ok(())
        }

        fn public_url(&self, key: &str) -> String {
            format!("https://media.example/{key}")
        }

        async fn signed_url(&self, key: &str, _expires_in: u32) -> PlatformResult<String> {
            Ok(self.public_url(key))
        }
    }

    #[derive(Default)]
    struct NoopHttp;

    #[async_trait(?Send)]
    impl HttpProvider for NoopHttp {
        async fn fetch(&self, _request: Request) -> PlatformResult<Response> {
            Err(PlatformError::Http("not used in core tests".to_string()))
        }
    }

    fn test_core(db: RecordingDb, queue: RecordingQueue) -> DaisCore {
        DaisCore::new(
            Box::new(db),
            Box::new(NoopStorage),
            Box::new(queue),
            Box::new(NoopHttp),
            CoreConfig {
                activitypub_domain: "social.example.com".to_string(),
                pds_domain: "pds.example.com".to_string(),
                username: "social".to_string(),
                private_key: "PRIVATE_KEY".to_string(),
                public_key: "PUBLIC_KEY".to_string(),
                media_url: "https://media.example.com".to_string(),
            },
        )
    }

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

    #[test]
    fn test_resolve_post_visibility_uses_private_default() {
        assert_eq!(
            resolve_post_visibility("", Some("followers")).unwrap(),
            "followers"
        );
    }

    #[test]
    fn test_resolve_post_visibility_honors_explicit_public() {
        assert_eq!(
            resolve_post_visibility("public", Some("followers")).unwrap(),
            "public"
        );
    }

    #[test]
    fn test_resolve_post_visibility_fails_closed_for_bad_default() {
        assert_eq!(
            resolve_post_visibility("", Some("not-valid")).unwrap(),
            "followers"
        );
    }

    #[test]
    fn test_resolve_post_visibility_rejects_bad_explicit_value() {
        assert!(resolve_post_visibility("not-valid", Some("followers")).is_err());
    }

    #[test]
    fn atproto_core_primitives_are_available() {
        let identity = crate::atproto::AtprotoIdentity::new(
            "did:web:pds.example",
            "social.example",
            "pds.example",
        );
        let snapshot = crate::atproto::RepoSnapshot {
            rev: "3lxyz".to_string(),
            commit_cid: "bafycommit".to_string(),
            car_bytes: vec![1, 2, 3],
        };
        let stats = crate::atproto::repo_stats(&snapshot);
        let uri =
            crate::atproto::record_uri(&identity.did, "app.bsky.feed.post", "20260704120000-test");
        let value = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "hello"
        });
        let record = crate::atproto::create_record_response(&uri, &value, &stats);
        let event = crate::atproto::commit_event(
            &identity,
            &stats,
            1,
            "2026-07-04T12:00:00Z",
            vec![crate::atproto::RepoOperation::create(
                crate::atproto::repo_path_from_at_uri(&uri).unwrap(),
                record.cid.clone(),
            )],
        );

        assert_eq!(crate::atproto::get_repo(&snapshot).unwrap(), vec![1, 2, 3]);
        assert_eq!(record.commit.cid, "bafycommit");
        assert_eq!(event.repo, identity.did);
        assert_eq!(event.ops[0].cid.as_deref(), Some(record.cid.as_str()));
    }

    #[tokio::test]
    async fn handle_commit_persists_metadata_and_queues_sync_message() {
        let db = RecordingDb::default();
        let queue = RecordingQueue::default();
        let core = test_core(db.clone(), queue.clone());

        core.handle_commit(
            "did:web:social.example.com".to_string(),
            "bafycommit".to_string(),
        )
        .await
        .unwrap();

        let calls = db.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert!(calls[0]
            .0
            .contains("INSERT OR IGNORE INTO atproto_sync_commits"));
        assert_eq!(calls[0].1[1].as_str(), Some("did:web:social.example.com"));
        assert_eq!(calls[0].1[2].as_str(), Some("bafycommit"));
        assert!(calls[1].0.contains("UPDATE atproto_sync_commits"));

        let messages = queue.messages.lock().unwrap().clone();
        assert_eq!(messages.len(), 1);
        let message: crate::traits::SyncMessage = serde_json::from_str(&messages[0]).unwrap();
        assert_eq!(message.pds_url, "https://pds.example.com");
        assert_eq!(message.repo_did, "did:web:social.example.com");
        assert_eq!(message.commit_cid, "bafycommit");
    }

    #[tokio::test]
    async fn subscribe_repos_queues_typed_core_request() {
        let db = RecordingDb::default();
        let queue = RecordingQueue::default();
        let core = test_core(db, queue.clone());

        core.subscribe_repos().await.unwrap();

        let messages = queue.messages.lock().unwrap().clone();
        assert_eq!(messages.len(), 1);
        let request: crate::atproto::SubscribeReposRequest =
            serde_json::from_str(&messages[0]).unwrap();
        assert_eq!(request.message_type, "atproto.sync.subscribeRepos");
        assert_eq!(request.pds_url, "https://pds.example.com");
        assert_eq!(request.repo_did, "did:web:social.example.com");
        assert_eq!(request.handle, "social.example.com");
        assert!(request.sequence_hint > 0);
    }
}
