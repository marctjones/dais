use worker::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct ATProtoActivity {
    text: String,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateRecordRequest {
    repo: String,
    collection: String,
    record: PostRecord,
}

#[derive(Debug, Serialize)]
struct PostRecord {
    text: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "$type")]
    record_type: String,
}

#[derive(Debug, Deserialize)]
struct SessionResponse {
    #[serde(rename = "accessJwt")]
    access_jwt: String,
    #[serde(rename = "refreshJwt")]
    refresh_jwt: String,
    handle: String,
    did: String,
}

/// Deliver a post to Bluesky via AT Protocol
pub async fn deliver_to_bluesky(
    activity_json: &str,
    env: &Env,
) -> Result<()> {
    // Parse the activity
    let activity: ATProtoActivity = serde_json::from_str(activity_json)
        .map_err(|e| Error::RustError(format!("Failed to parse activity: {}", e)))?;

    // Get Bluesky credentials from secrets
    let handle = env.secret("BLUESKY_HANDLE")
        .map_err(|_| Error::RustError("BLUESKY_HANDLE secret not found".to_string()))?
        .to_string();

    let password = env.secret("BLUESKY_PASSWORD")
        .map_err(|_| Error::RustError("BLUESKY_PASSWORD secret not found".to_string()))?
        .to_string();

    // Create session with Bluesky
    let session = create_session(&handle, &password).await?;

    // Create the post record
    let created_at = activity.created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let record = PostRecord {
        text: activity.text.clone(),
        created_at,
        record_type: "app.bsky.feed.post".to_string(),
    };

    let create_request = CreateRecordRequest {
        repo: session.did.clone(),
        collection: "app.bsky.feed.post".to_string(),
        record,
    };

    // Send the post to Bluesky
    let mut headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", session.access_jwt))?;
    headers.set("Content-Type", "application/json")?;

    let body = serde_json::to_string(&create_request)
        .map_err(|e| Error::RustError(format!("Failed to serialize request: {}", e)))?;

    let mut request_init = RequestInit::new();
    request_init
        .with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.into()));

    let request = Request::new_with_init(
        "https://bsky.social/xrpc/com.atproto.repo.createRecord",
        &request_init
    )?;

    let mut response = Fetch::Request(request).send().await?;

    if response.status_code() >= 200 && response.status_code() < 300 {
        let response_json: serde_json::Value = response.json().await?;
        console_log!("✓ Posted to Bluesky: {:?}", response_json);
        Ok(())
    } else {
        let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(Error::RustError(format!(
            "Bluesky API error {}: {}",
            response.status_code(),
            error_body
        )))
    }
}

/// Create a session with Bluesky and get access token
async fn create_session(handle: &str, password: &str) -> Result<SessionResponse> {
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json")?;

    let body = serde_json::json!({
        "identifier": handle,
        "password": password
    }).to_string();

    let mut request_init = RequestInit::new();
    request_init
        .with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.into()));

    let request = Request::new_with_init(
        "https://bsky.social/xrpc/com.atproto.server.createSession",
        &request_init
    )?;

    let mut response = Fetch::Request(request).send().await?;

    if response.status_code() >= 200 && response.status_code() < 300 {
        let session: SessionResponse = response.json().await?;
        Ok(session)
    } else {
        let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(Error::RustError(format!(
            "Bluesky auth failed {}: {}",
            response.status_code(),
            error_body
        )))
    }
}
