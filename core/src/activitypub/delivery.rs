/// ActivityPub activity delivery to remote inboxes
///
/// Handles signing and delivering activities to followers

use crate::traits::{DatabaseProvider, HttpProvider};
use crate::error::{CoreResult, CoreError};
use crate::activitypub::signatures;
use std::collections::HashMap;
use serde_json::Value;

/// Delivery job information
#[derive(Debug, Clone)]
pub struct DeliveryJob {
    pub delivery_id: String,
    pub post_id: String,
    pub target_url: String,
    pub actor_url: String,
    pub activity_json: String,
    pub retry_count: u32,
}

/// Get all follower inbox URLs for an actor
pub async fn get_follower_inboxes(
    db: &dyn DatabaseProvider,
    actor_id: &str,
) -> CoreResult<Vec<String>> {
    let query = r#"
        SELECT follower_inbox
        FROM followers
        WHERE actor_id = ?1 AND status = 'approved'
    "#;

    let rows = db.execute(query, &[Value::String(actor_id.to_string())]).await?;

    let mut inboxes = Vec::new();
    for row in rows {
        if let Some(inbox) = row.get("follower_inbox").and_then(|v| v.as_str()) {
            inboxes.push(inbox.to_string());
        }
    }

    Ok(inboxes)
}

/// Deliver an activity to a remote inbox with HTTP signatures
pub async fn deliver_to_inbox(
    http: &dyn HttpProvider,
    inbox_url: &str,
    actor_url: &str,
    activity_json: &str,
    private_key_pem: &str,
) -> CoreResult<()> {
    // Parse inbox URL to get host and path
    let url = url::Url::parse(inbox_url)
        .map_err(|e| CoreError::InvalidActivity(format!("Invalid inbox URL: {}", e)))?;

    let host = url.host_str()
        .ok_or_else(|| CoreError::InvalidActivity("No host in inbox URL".to_string()))?;
    let path = url.path();

    // Generate Date header
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    // Generate Digest header (SHA-256 of body)
    use sha2::{Sha256, Digest as Sha2Digest};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let body_hash = Sha256::digest(activity_json.as_bytes());
    let digest = format!("SHA-256={}", BASE64.encode(&body_hash));

    // Build headers for signing
    let mut sign_headers = HashMap::new();
    sign_headers.insert("host".to_string(), host.to_string());
    sign_headers.insert("date".to_string(), date.clone());
    sign_headers.insert("digest".to_string(), digest.clone());

    // Headers to include in signature
    let headers_to_sign = vec![
        "(request-target)".to_string(),
        "host".to_string(),
        "date".to_string(),
        "digest".to_string(),
    ];

    // Generate HTTP signature
    let key_id = format!("{}#main-key", actor_url);

    let http_signature = signatures::sign_request(
        private_key_pem,
        &key_id,
        "POST",
        path,
        &sign_headers,
        &headers_to_sign,
    ).map_err(|e| CoreError::SignatureError(e))?;

    // Build request headers
    let mut headers = HashMap::new();
    headers.insert("Host".to_string(), host.to_string());
    headers.insert("Date".to_string(), date);
    headers.insert("Digest".to_string(), digest);
    headers.insert("Signature".to_string(), http_signature.to_header());
    headers.insert("Content-Type".to_string(), "application/activity+json".to_string());
    headers.insert("User-Agent".to_string(), "dais/1.1.0".to_string());

    // Make the HTTP POST request
    let request = crate::traits::Request {
        url: inbox_url.to_string(),
        method: crate::traits::Method::Post,
        headers,
        body: Some(activity_json.as_bytes().to_vec()),
        timeout: Some(30),
        follow_redirects: false,
    };

    let response = http.fetch(request).await?;

    if response.status >= 200 && response.status < 300 {
        Ok(())
    } else {
        let error_body = String::from_utf8(response.body)
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(CoreError::Internal(format!(
            "HTTP {} from {}: {}",
            response.status,
            inbox_url,
            error_body
        )))
    }
}

/// Create delivery jobs for all followers
pub async fn create_follower_deliveries(
    db: &dyn DatabaseProvider,
    post_id: &str,
    actor_id: &str,
    activity_json: &str,
) -> CoreResult<Vec<String>> {
    // Get all follower inboxes
    let inboxes = get_follower_inboxes(db, actor_id).await?;

    let mut delivery_ids = Vec::new();

    for inbox_url in inboxes {
        // Create delivery record
        let delivery_id = crate::utils::generate_uuid();
        let created_at = crate::utils::now_rfc3339();

        let query = r#"
            INSERT INTO deliveries (
                id, post_id, target_type, target_url, actor_id,
                activity_json, status, retry_count, created_at
            ) VALUES (?1, ?2, 'inbox', ?3, ?4, ?5, 'pending', 0, ?6)
        "#;

        db.execute(query, &[
            Value::String(delivery_id.clone()),
            Value::String(post_id.to_string()),
            Value::String(inbox_url),
            Value::String(actor_id.to_string()),
            Value::String(activity_json.to_string()),
            Value::String(created_at),
        ]).await?;

        delivery_ids.push(delivery_id);
    }

    Ok(delivery_ids)
}

/// Update delivery status after attempt
pub async fn update_delivery_status(
    db: &dyn DatabaseProvider,
    delivery_id: &str,
    success: bool,
    error_message: Option<&str>,
    retry_count: u32,
) -> CoreResult<()> {
    let now = crate::utils::now_rfc3339();

    if success {
        let query = r#"
            UPDATE deliveries
            SET status = 'delivered', delivered_at = ?1, last_attempt_at = ?2
            WHERE id = ?3
        "#;

        db.execute(query, &[
            Value::String(now.clone()),
            Value::String(now),
            Value::String(delivery_id.to_string()),
        ]).await?;
    } else {
        let new_status = if retry_count >= 3 { "failed" } else { "retry" };

        let query = r#"
            UPDATE deliveries
            SET status = ?1, retry_count = ?2, last_attempt_at = ?3, error_message = ?4
            WHERE id = ?5
        "#;

        db.execute(query, &[
            Value::String(new_status.to_string()),
            Value::Number(serde_json::Number::from(retry_count + 1)),
            Value::String(now),
            error_message.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
            Value::String(delivery_id.to_string()),
        ]).await?;
    }

    Ok(())
}
