use async_trait::async_trait;
use dais_cloudflare::{D1Provider, WorkerHttpProvider};
use dais_core::{CoreConfig, DaisCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
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

#[derive(Debug, Deserialize, Serialize)]
pub struct DeliveryMessage {
    delivery_id: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryProcessRequest {
    delivery_id: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryEnqueueRequest {
    delivery_id: String,
}

#[derive(Debug, Deserialize)]
struct FollowerAcceptRequest {
    actor_id: Option<String>,
    follower_actor_id: String,
}

#[derive(Debug, Serialize)]
struct DeliveryProcessReport {
    delivery_id: String,
    success: bool,
    retryable: bool,
    retry_count: u32,
}

#[derive(Debug, Serialize)]
struct DeliveryEnqueueReport {
    delivery_id: String,
    enqueued: bool,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct FollowerAcceptReport {
    follower_actor_id: String,
    accepted: bool,
    inbox: String,
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .post_async("/deliveries/enqueue", handle_enqueue_delivery)
        .post_async("/deliveries/process", handle_process_delivery)
        .post_async("/followers/accept", handle_follower_accept)
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

async fn handle_enqueue_delivery(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body = req.text().await?;
    let request: DeliveryEnqueueRequest = serde_json::from_str(&body)
        .map_err(|_| worker::Error::RustError("Invalid JSON body".to_string()))?;
    if !is_delivery_id(&request.delivery_id) {
        return Response::error("Invalid delivery id", 400);
    }

    let db = ctx.env.d1("DB")?;
    let rows = db
        .prepare("SELECT status FROM deliveries WHERE id = ?1 LIMIT 1")
        .bind(&[request.delivery_id.clone().into()])?
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;

    let Some(row) = rows.into_iter().next() else {
        return Response::error("Delivery not found", 404);
    };
    let status = row
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    if status != "queued" && status != "retry" {
        let mut resp = Response::from_json(&DeliveryEnqueueReport {
            delivery_id: request.delivery_id,
            enqueued: false,
            status: Some(status),
        })?;
        resp.headers_mut().set("Content-Type", "application/json")?;
        return Ok(resp);
    }

    let queue = ctx.env.queue("DELIVERY_QUEUE")?;
    queue
        .send(DeliveryMessage {
            delivery_id: request.delivery_id.clone(),
        })
        .await?;

    let mut resp = Response::from_json(&DeliveryEnqueueReport {
        delivery_id: request.delivery_id,
        enqueued: true,
        status: Some(status),
    })?;
    resp.headers_mut().set("Content-Type", "application/json")?;
    Ok(resp)
}

async fn handle_follower_accept(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body = req.text().await?;
    let request: FollowerAcceptRequest = serde_json::from_str(&body)
        .map_err(|_| worker::Error::RustError("Invalid JSON body".to_string()))?;
    let actor_id = request
        .actor_id
        .unwrap_or_else(|| "https://social.dais.social/users/social".to_string());

    let db = ctx.env.d1("DB")?;
    let rows = db
        .prepare(
            r#"
            SELECT id, follower_inbox, follower_shared_inbox, status
            FROM followers
            WHERE actor_id = ?1 AND follower_actor_id = ?2
            LIMIT 1
            "#,
        )
        .bind(&[
            actor_id.clone().into(),
            request.follower_actor_id.clone().into(),
        ])?
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;

    let Some(row) = rows.into_iter().next() else {
        return Response::error("Follower not found", 404);
    };
    let status = row
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if status != "approved" {
        return Response::error("Follower is not approved", 409);
    }

    let follow_id = row
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or(&request.follower_actor_id);
    let inbox = row
        .get("follower_shared_inbox")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .or_else(|| row.get("follower_inbox").and_then(|value| value.as_str()))
        .unwrap_or("");
    if inbox.is_empty() {
        return Response::error("Follower inbox not found", 500);
    }

    let private_key = ctx
        .env
        .secret("PRIVATE_KEY")
        .map(|s| s.to_string())
        .unwrap_or_default();
    if private_key.trim().is_empty() {
        return Response::error("Private key not configured", 500);
    }

    let accept = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{actor_id}#accepts/{}", accept_suffix(follow_id)),
        "type": "Accept",
        "actor": actor_id,
        "to": [request.follower_actor_id.clone()],
        "object": {
            "id": follow_id,
            "type": "Follow",
            "actor": request.follower_actor_id,
            "object": actor_id
        }
    });

    let http = WorkerHttpProvider::new();
    dais_core::activitypub::deliver_to_inbox(
        &http,
        inbox,
        &actor_id,
        &accept.to_string(),
        &private_key,
    )
    .await
    .map_err(|error| worker::Error::RustError(error.to_string()))?;

    let mut resp = Response::from_json(&FollowerAcceptReport {
        follower_actor_id: accept
            .get("to")
            .and_then(|value| value.as_array())
            .and_then(|values| values.first())
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        accepted: true,
        inbox: inbox.to_string(),
    })?;
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
            d.activity_json,
            p.actor_id,
            p.content,
            p.content_html,
            COALESCE(p.object_type, 'Note') AS object_type,
            p.name,
            p.summary,
            p.end_time,
            p.poll_options,
            p.visibility,
            p.published_at,
            p.encrypted_message,
            p.in_reply_to,
            p.media_attachments,
            (
                SELECT follower_actor_id
                FROM followers f
                WHERE f.status = 'approved'
                  AND (f.follower_inbox = d.target_url OR f.follower_shared_inbox = d.target_url)
                ORDER BY updated_at DESC
                LIMIT 1
            ) AS delivery_recipient
        FROM deliveries d
        LEFT JOIN posts p ON p.id = d.post_id
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
    let stored_activity_json = row.get("activity_json").and_then(|v| v.as_str());
    let stored_activity = stored_activity_json
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    let actor_id = row
        .get("actor_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            stored_activity
                .as_ref()
                .and_then(|activity| activity.get("actor"))
                .and_then(|value| value.as_str())
        })
        .unwrap_or("");
    let post_id = row.get("post_id").and_then(|v| v.as_str()).unwrap_or("");
    let content = row.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let content_html = row
        .get("content_html")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let object_type = row
        .get("object_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Note");
    let name = row.get("name").and_then(|v| v.as_str());
    let summary = row.get("summary").and_then(|v| v.as_str());
    let end_time = row.get("end_time").and_then(|v| v.as_str());
    let poll_options = row.get("poll_options").and_then(|v| v.as_str());
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
    let media_attachments = row.get("media_attachments").and_then(|v| v.as_str());
    let delivery_recipient = row.get("delivery_recipient").and_then(|v| v.as_str());
    let retry_count = row.get("retry_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    console_log!("Delivering to: {}", target_url);
    if !dais_core::activitypub::is_federation_host_allowed(core.db(), target_url)
        .await
        .map_err(|error| worker::Error::RustError(error.to_string()))?
    {
        if let Err(update_err) = dais_core::activitypub::update_delivery_status(
            core.db(),
            delivery_id,
            false,
            Some("delivery target is not allowlisted while closed_network is enabled"),
            3,
        )
        .await
        {
            console_log!("Failed to update delivery status: {}", update_err);
        }
        return Ok(DeliveryProcessReport {
            delivery_id: delivery_id.to_string(),
            success: false,
            retryable: false,
            retry_count,
        });
    }

    let activity_json = match stored_activity_json {
        Some(value) if !value.trim().is_empty() => value.to_string(),
        _ => build_create_activity_json(
            actor_id,
            post_id,
            content,
            content_html,
            object_type,
            name,
            summary,
            end_time,
            poll_options,
            visibility,
            published_at,
            encrypted_message,
            in_reply_to,
            media_attachments,
            delivery_recipient,
        )
        .map_err(worker::Error::RustError)?,
    };
    let extra_headers =
        mastodon_collection_synchronization_headers(core, actor_id, target_url, visibility)
            .await
            .map_err(worker::Error::RustError)?;

    let result = core
        .deliver_to_inbox_with_extra_headers(
            target_url.to_string(),
            actor_id.to_string(),
            activity_json.to_string(),
            extra_headers,
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

fn is_delivery_id(value: &str) -> bool {
    value.strip_prefix("delivery-").is_some_and(|suffix| {
        suffix.len() >= 5
            && suffix.len() <= 32
            && suffix
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    })
}

fn accept_suffix(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .take(24)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn mastodon_collection_synchronization_headers(
    core: &DaisCore,
    actor_id: &str,
    target_url: &str,
    visibility: &str,
) -> Result<Vec<(String, String)>, String> {
    if visibility != "followers" {
        return Ok(Vec::new());
    }
    let target_host = url::Url::parse(target_url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()));
    let Some(target_host) = target_host else {
        return Ok(Vec::new());
    };
    let rows = core
        .db()
        .execute(
            r#"
            SELECT follower_actor_id
            FROM followers
            WHERE actor_id = ?1 AND status = 'approved'
            ORDER BY follower_actor_id ASC
            "#,
            &[Value::String(actor_id.to_string())],
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut actor_ids = rows
        .iter()
        .filter_map(|row| {
            row.get("follower_actor_id")
                .and_then(|value| value.as_str())
        })
        .filter(|actor| actor_host(actor).as_deref() == Some(target_host.as_str()))
        .map(str::to_string)
        .collect::<Vec<_>>();
    actor_ids.sort();
    actor_ids.dedup();
    if actor_ids.is_empty() {
        return Ok(Vec::new());
    }
    let digest = xor_sha256_hex(&actor_ids);
    let url = format!("{actor_id}/followers_synchronization?domain={target_host}");
    let value = format!(r#"collectionId="{actor_id}/followers", url="{url}", digest="{digest}""#);
    Ok(vec![("Collection-Synchronization".to_string(), value)])
}

fn actor_host(actor_id: &str) -> Option<String> {
    url::Url::parse(actor_id)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
}

fn xor_sha256_hex(values: &[String]) -> String {
    let mut digest = [0_u8; 32];
    for value in values {
        let hash = Sha256::digest(value.as_bytes());
        for (index, byte) in hash.iter().enumerate() {
            digest[index] ^= byte;
        }
    }
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn build_create_activity_json(
    actor_id: &str,
    post_id: &str,
    content: &str,
    content_html: &str,
    object_type: &str,
    name: Option<&str>,
    summary: Option<&str>,
    end_time: Option<&str>,
    poll_options: Option<&str>,
    visibility: &str,
    published_at: &str,
    encrypted_message: Option<serde_json::Value>,
    in_reply_to: Option<&str>,
    media_attachments: Option<&str>,
    delivery_recipient: Option<&str>,
) -> Result<String, String> {
    let followers_collection = format!("{actor_id}/followers");
    let to = activity_to(visibility, &followers_collection, delivery_recipient);
    let cc = activity_cc(visibility, &followers_collection);

    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": object_type,
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

    if let Some(name) = name {
        note["name"] = serde_json::json!(name);
    }

    if let Some(summary) = summary {
        note["summary"] = serde_json::json!(summary);
    }

    if let Some(end_time) = end_time {
        note["endTime"] = serde_json::json!(end_time);
    }

    if let Some(in_reply_to) = in_reply_to {
        note["inReplyTo"] = serde_json::json!(in_reply_to);
    }

    if let Some(media_attachments) = media_attachments {
        if let Ok(attachments) = serde_json::from_str::<serde_json::Value>(media_attachments) {
            note["attachment"] = attachments;
        }
    }

    if let Some(poll_options) = poll_options {
        if let Some((multiple, options)) = parse_poll_options(poll_options) {
            let key = if multiple { "anyOf" } else { "oneOf" };
            note[key] = serde_json::json!(poll_option_values(&options));
            note["votersCount"] = serde_json::json!(0);
        }
    }

    let tags = activity_tags(content);
    if !tags.is_empty() {
        note["tag"] = serde_json::json!(tags);
    }

    if let Some(encrypted_message) = encrypted_message {
        note["daisEncryptedMessage"] = encrypted_message;
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

fn activity_tags(content: &str) -> Vec<serde_json::Value> {
    let mut tags = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in content.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\''
            )
        });
        if let Some(tag) = hashtag_tag(trimmed) {
            if seen.insert(format!("hashtag:{trimmed}")) {
                tags.push(tag);
            }
            continue;
        }
        if let Some(tag) = mention_tag(trimmed) {
            if seen.insert(format!("mention:{trimmed}")) {
                tags.push(tag);
            }
        }
    }
    tags
}

fn parse_poll_options(value: &str) -> Option<(bool, Vec<String>)> {
    let value = serde_json::from_str::<serde_json::Value>(value).ok()?;
    if let Some(options) = value.as_array() {
        return Some((
            false,
            options
                .iter()
                .filter_map(|option| option.as_str().map(ToString::to_string))
                .collect(),
        ));
    }

    let multiple = value
        .get("multiple")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let options = value
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|option| option.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();
    Some((multiple, options))
}

fn poll_option_values(options: &[String]) -> Vec<serde_json::Value> {
    options
        .iter()
        .map(|option| {
            serde_json::json!({
                "type": "Note",
                "name": option.trim(),
                "replies": {
                    "type": "Collection",
                    "totalItems": 0
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{actor_host, is_delivery_id, xor_sha256_hex};

    #[test]
    fn delivery_id_accepts_owner_api_generated_ids() {
        assert!(is_delivery_id("delivery-34eqe"));
        assert!(is_delivery_id("delivery-1398fgt"));
        assert!(is_delivery_id("delivery-1234567890abcdef"));
    }

    #[test]
    fn delivery_id_rejects_malformed_values() {
        assert!(!is_delivery_id("34eqe"));
        assert!(!is_delivery_id("delivery-"));
        assert!(!is_delivery_id("delivery-abcd"));
        assert!(!is_delivery_id("delivery-ABCDEF"));
        assert!(!is_delivery_id("delivery-abc_def"));
        assert!(!is_delivery_id(
            "delivery-123456789012345678901234567890123"
        ));
    }

    #[test]
    fn actor_host_extracts_https_domain() {
        assert_eq!(
            actor_host("https://mastodon.social/users/alice").as_deref(),
            Some("mastodon.social")
        );
        assert_eq!(actor_host("not a url"), None);
    }

    #[test]
    fn xor_sha256_hex_is_order_independent_when_inputs_sorted() {
        let mut actors = vec![
            "https://mastodon.social/users/bob".to_string(),
            "https://mastodon.social/users/alice".to_string(),
        ];
        actors.sort();
        let first = xor_sha256_hex(&actors);
        let second = xor_sha256_hex(&actors);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}

fn hashtag_tag(token: &str) -> Option<serde_json::Value> {
    let name = token.strip_prefix('#')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(serde_json::json!({
        "type": "Hashtag",
        "name": format!("#{name}"),
        "href": format!("https://social.dais.social/tags/{name}")
    }))
}

fn mention_tag(token: &str) -> Option<serde_json::Value> {
    let without_prefix = token.strip_prefix('@')?;
    let mut parts = without_prefix.split('@');
    let username = parts.next()?.trim();
    let host = parts.next()?.trim();
    if parts.next().is_some()
        || username.is_empty()
        || host.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
    {
        return None;
    }
    Some(serde_json::json!({
        "type": "Mention",
        "name": format!("@{username}@{host}"),
        "href": format!("https://{host}/users/{username}")
    }))
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
        _ => delivery_recipient
            .map(|recipient| vec![followers_collection.to_string(), recipient.to_string()])
            .unwrap_or_else(|| vec![followers_collection.to_string()]),
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
