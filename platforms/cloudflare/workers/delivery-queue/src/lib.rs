use async_trait::async_trait;
use dais_cloudflare::{D1Provider, WorkerHttpProvider};
use dais_core::{CoreConfig, DaisCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug, Deserialize)]
pub struct DeliveryMessage {
    delivery_id: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryProcessRequest {
    delivery_id: String,
}

#[derive(Debug, Serialize)]
struct DeliveryProcessReport {
    delivery_id: String,
    success: bool,
    retryable: bool,
    retry_count: u32,
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .post_async("/deliveries/process", handle_process_delivery)
        .run(req, env)
        .await
}

#[event(queue)]
pub async fn main(
    message_batch: MessageBatch<DeliveryMessage>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    console_error_panic_hook::set_once();

    // Get database and HTTP provider
    let db = D1Provider::new(env.d1("DB")?);
    let http = WorkerHttpProvider::new();

    // Get configuration from environment
    let configured_domain = env
        .var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    let username = env
        .var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    let private_key = env
        .secret("PRIVATE_KEY")
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

        match process_delivery(&core, delivery_id).await {
            Ok(report) => {
                if report.success {
                    console_log!("✓ Delivery {} successful", delivery_id);
                    msg.ack();
                } else if report.retryable {
                    console_log!(
                        "Retrying delivery {} (attempt {})",
                        delivery_id,
                        report.retry_count + 1
                    );
                    msg.retry();
                } else {
                    console_log!(
                        "Max retries exceeded for delivery {}, marking as failed",
                        delivery_id
                    );
                    msg.ack();
                }
            }
            Err(e) => {
                console_log!("✗ Delivery {} failed: {}", delivery_id, e);
                msg.retry();
            }
        }
    }

    Ok(())
}

async fn handle_process_delivery(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let admin_token = ctx
        .env
        .secret("DELIVERY_ADMIN_TOKEN")
        .map(|s| s.to_string())
        .unwrap_or_default();

    if admin_token.is_empty() {
        return Response::error("Delivery admin token not configured", 500);
    }

    let provided_token = req.headers().get("X-Dais-Admin-Token")?.unwrap_or_default();
    if provided_token != admin_token {
        return Response::error("Unauthorized", 401);
    }

    let body = req.text().await?;
    let request: DeliveryProcessRequest = serde_json::from_str(&body)
        .map_err(|_| worker::Error::RustError("Invalid JSON body".to_string()))?;

    let db = D1Provider::new(ctx.env.d1("DB")?);
    let http = WorkerHttpProvider::new();

    let configured_domain = ctx
        .env
        .var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = ctx
        .env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    let username = ctx
        .env
        .var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    let private_key = ctx
        .env
        .secret("PRIVATE_KEY")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| {
            console_log!("WARNING: PRIVATE_KEY secret not found");
            String::new()
        });

    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(http),
        CoreConfig {
            activitypub_domain,
            pds_domain: "".to_string(),
            username,
            private_key,
            public_key: "".to_string(),
            media_url: "".to_string(),
        },
    );

    let report = process_delivery(&core, &request.delivery_id)
        .await
        .map_err(|e| worker::Error::RustError(e.to_string()))?;

    let mut resp = Response::from_json(&report)?;
    resp.headers_mut().set("Content-Type", "application/json")?;
    Ok(resp)
}

async fn process_delivery(
    core: &DaisCore,
    delivery_id: &str,
) -> Result<DeliveryProcessReport, worker::Error> {
    let query = r#"
        SELECT
            d.id,
            d.target_url,
            d.post_id,
            d.retry_count,
            p.actor_id,
            p.content,
            p.content_html,
            p.visibility,
            p.published_at,
            p.encrypted_message,
            p.in_reply_to,
            f.follower_actor_id AS delivery_recipient
        FROM deliveries d
        JOIN posts p ON p.id = d.post_id
        LEFT JOIN followers f ON f.follower_inbox = d.target_url
        WHERE d.id = ?1 AND d.status IN ('queued', 'retry')
    "#;

    let rows = core
        .db()
        .execute(query, &[Value::String(delivery_id.to_string())])
        .await
        .map_err(|e| {
            worker::Error::RustError(format!(
                "Database error fetching delivery {}: {}",
                delivery_id, e
            ))
        })?;

    if rows.is_empty() {
        return Ok(DeliveryProcessReport {
            delivery_id: delivery_id.to_string(),
            success: false,
            retryable: false,
            retry_count: 0,
        });
    }

    let row = &rows[0];
    let target_url = row.get("target_url").and_then(|v| v.as_str()).unwrap_or("");
    let actor_id = row.get("actor_id").and_then(|v| v.as_str()).unwrap_or("");
    let post_id = row.get("post_id").and_then(|v| v.as_str()).unwrap_or("");
    let content = row.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let content_html = row
        .get("content_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let visibility = row
        .get("visibility")
        .and_then(|v| v.as_str())
        .unwrap_or("followers");
    let published_at = row
        .get("published_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let encrypted_message = row
        .get("encrypted_message")
        .and_then(|v| v.as_str())
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok());
    let in_reply_to = row.get("in_reply_to").and_then(|v| v.as_str());
    let delivery_recipient = row.get("delivery_recipient").and_then(|v| v.as_str());
    let retry_count = row.get("retry_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    console_log!("Delivering to: {}", target_url);

    let activity_json = build_create_activity_json(
        actor_id,
        post_id,
        content,
        content_html,
        visibility,
        published_at,
        encrypted_message,
        in_reply_to,
        delivery_recipient,
    )
    .map_err(worker::Error::RustError)?;

    let result = core
        .deliver_to_inbox(
            target_url.to_string(),
            actor_id.to_string(),
            activity_json.to_string(),
        )
        .await;

    match result {
        Ok(()) => {
            if let Err(e) = dais_core::activitypub::update_delivery_status(
                core.db(),
                delivery_id,
                true,
                None,
                retry_count,
            )
            .await
            {
                console_log!("Failed to update delivery status: {}", e);
            }

            Ok(DeliveryProcessReport {
                delivery_id: delivery_id.to_string(),
                success: true,
                retryable: false,
                retry_count,
            })
        }
        Err(e) => {
            if let Err(update_err) = dais_core::activitypub::update_delivery_status(
                core.db(),
                delivery_id,
                false,
                Some(&e.to_string()),
                retry_count,
            )
            .await
            {
                console_log!("Failed to update delivery status: {}", update_err);
            }

            Ok(DeliveryProcessReport {
                delivery_id: delivery_id.to_string(),
                success: false,
                retryable: retry_count < 3,
                retry_count,
            })
        }
    }
}

fn build_create_activity_json(
    actor_id: &str,
    post_id: &str,
    content: &str,
    content_html: &str,
    visibility: &str,
    published_at: &str,
    encrypted_message: Option<serde_json::Value>,
    in_reply_to: Option<&str>,
    delivery_recipient: Option<&str>,
) -> Result<String, String> {
    let followers_collection = format!("{actor_id}/followers");
    let to = activity_to(visibility, &followers_collection, delivery_recipient);
    let cc = activity_cc(visibility, &followers_collection);

    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": post_id,
        "attributedTo": actor_id,
        "content": content,
        "published": published_at,
        "to": to
    });

    if !cc.is_empty() {
        note["cc"] = serde_json::json!(cc);
    }

    if !content_html.is_empty() {
        note["contentMap"] = serde_json::json!({ "en": content_html });
    }

    if let Some(in_reply_to) = in_reply_to {
        note["inReplyTo"] = serde_json::json!(in_reply_to);
    }

    if let Some(encrypted_message) = encrypted_message {
        note["encryptedMessage"] = encrypted_message;
    }

    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": format!("{post_id}#create"),
        "actor": actor_id,
        "published": published_at,
        "to": note["to"].clone(),
        "cc": note.get("cc").cloned().unwrap_or_else(|| serde_json::json!([])),
        "object": note
    });

    serde_json::to_string(&activity).map_err(|e| e.to_string())
}

fn activity_to(
    visibility: &str,
    followers_collection: &str,
    delivery_recipient: Option<&str>,
) -> Vec<String> {
    match visibility {
        "public" | "unlisted" => vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
        "direct" => delivery_recipient
            .map(|recipient| vec![recipient.to_string()])
            .unwrap_or_default(),
        _ => vec![followers_collection.to_string()],
    }
}

fn activity_cc(visibility: &str, followers_collection: &str) -> Vec<String> {
    match visibility {
        "public" | "unlisted" => vec![followers_collection.to_string()],
        _ => Vec::new(),
    }
}

// Placeholder providers for unused platform features

use dais_core::traits::{
    ListOptions, ListResult, ObjectInfo, PlatformError, PlatformResult, QueueProvider,
    StorageMetadata, StorageProvider,
};

struct PlaceholderStorage;

#[async_trait(?Send)]
impl StorageProvider for PlaceholderStorage {
    async fn put(&self, _key: &str, _data: Vec<u8>, _content_type: &str) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn put_with_metadata(
        &self,
        _key: &str,
        _data: Vec<u8>,
        _content_type: &str,
        _metadata: StorageMetadata,
    ) -> PlatformResult<String> {
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
