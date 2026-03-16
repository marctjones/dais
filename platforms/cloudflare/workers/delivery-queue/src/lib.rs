/// Refactored Delivery Queue worker using dais-core
///
/// This is a thin shim that:
/// 1. Receives delivery jobs from Cloudflare Queue
/// 2. Retrieves delivery information from database
/// 3. Calls core.deliver_to_inbox() for HTTP signature signing and delivery
///
/// All delivery logic (signature generation, HTTP POST, retry handling) is
/// now in dais-core, making it reusable across platforms.

use worker::*;
use dais_cloudflare::{D1Provider, WorkerHttpProvider};
use dais_core::{DaisCore, CoreConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use async_trait::async_trait;

#[derive(Debug, Deserialize)]
struct DeliveryMessage {
    delivery_id: String,
}

#[event(queue)]
pub async fn main(message_batch: MessageBatch<DeliveryMessage>, env: Env, _ctx: Context) -> Result<()> {
    console_error_panic_hook::set_once();

    // Get database and HTTP provider
    let db = D1Provider::new(env.d1("DB")?);
    let http = WorkerHttpProvider::new();

    // Get configuration from environment
    let configured_domain = env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    let username = env.var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    let private_key = env.secret("PRIVATE_KEY")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| {
            console_log!("WARNING: PRIVATE_KEY secret not found");
            String::new()
        });

    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),
        username,
        private_key,
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    // Initialize DaisCore
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(http),
        config,
    );

    // Process each delivery job in the batch
    for msg in message_batch.messages()? {
        let delivery_id = &msg.body().delivery_id;
        console_log!("Processing delivery: {}", delivery_id);

        // Retrieve delivery job from database
        let query = r#"
            SELECT id, target_url, actor_id, activity_json, retry_count
            FROM deliveries
            WHERE id = ?1 AND status IN ('pending', 'retry')
        "#;

        let rows = match core.db().execute(query, &[Value::String(delivery_id.clone())]).await {
            Ok(r) => r,
            Err(e) => {
                console_log!("Database error fetching delivery {}: {}", delivery_id, e);
                msg.retry();
                continue;
            }
        };

        if rows.is_empty() {
            console_log!("Delivery {} not found or already delivered", delivery_id);
            msg.ack();
            continue;
        }

        let row = &rows[0];
        let target_url = row.get("target_url").and_then(|v| v.as_str()).unwrap_or("");
        let actor_id = row.get("actor_id").and_then(|v| v.as_str()).unwrap_or("");
        let activity_json = row.get("activity_json").and_then(|v| v.as_str()).unwrap_or("");
        let retry_count = row.get("retry_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        // Build actor URL
        let actor_url = actor_id.to_string();

        console_log!("Delivering to: {}", target_url);

        // Call core delivery function
        let result = core.deliver_to_inbox(
            target_url.to_string(),
            actor_url,
            activity_json.to_string(),
        ).await;

        match result {
            Ok(()) => {
                console_log!("✓ Delivery {} successful", delivery_id);

                // Update delivery status to 'delivered'
                if let Err(e) = dais_core::activitypub::update_delivery_status(
                    core.db(),
                    delivery_id,
                    true,
                    None,
                    retry_count,
                ).await {
                    console_log!("Failed to update delivery status: {}", e);
                }

                msg.ack();
            }
            Err(e) => {
                console_log!("✗ Delivery {} failed: {}", delivery_id, e);

                // Update delivery status to 'retry' or 'failed'
                if let Err(update_err) = dais_core::activitypub::update_delivery_status(
                    core.db(),
                    delivery_id,
                    false,
                    Some(&e.to_string()),
                    retry_count,
                ).await {
                    console_log!("Failed to update delivery status: {}", update_err);
                }

                // Retry if we haven't exceeded max retries
                if retry_count < 3 {
                    console_log!("Retrying delivery {} (attempt {})", delivery_id, retry_count + 1);
                    msg.retry();
                } else {
                    console_log!("Max retries exceeded for delivery {}, marking as failed", delivery_id);
                    msg.ack();
                }
            }
        }
    }

    Ok(())
}

// Placeholder providers for unused platform features

use dais_core::traits::{
    StorageProvider, QueueProvider, PlatformResult, PlatformError,
    StorageMetadata, ObjectInfo, ListOptions, ListResult,
};

struct PlaceholderStorage;

#[async_trait(?Send)]
impl StorageProvider for PlaceholderStorage {
    async fn put(&self, _key: &str, _data: Vec<u8>, _content_type: &str) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn put_with_metadata(&self, _key: &str, _data: Vec<u8>, _content_type: &str, _metadata: StorageMetadata) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn get(&self, _key: &str) -> PlatformResult<Vec<u8>> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn head(&self, _key: &str) -> PlatformResult<ObjectInfo> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn delete(&self, _key: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn list(&self, _prefix: &str) -> PlatformResult<Vec<String>> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn list_detailed(&self, _options: ListOptions) -> PlatformResult<ListResult> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn copy(&self, _from: &str, _to: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    fn public_url(&self, _key: &str) -> String {
        String::new()
    }

    async fn signed_url(&self, _key: &str, _expires_in: u32) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }
}

struct PlaceholderQueue;

#[async_trait(?Send)]
impl QueueProvider for PlaceholderQueue {
    async fn send(&self, _message: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn send_batch(&self, _messages: Vec<String>) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn send_delayed(&self, _message: &str, _delay_seconds: u32) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn depth(&self) -> PlatformResult<u64> {
        Ok(0)
    }
}
