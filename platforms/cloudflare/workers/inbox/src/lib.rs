/// Refactored Inbox worker using dais-core
///
/// This is a thin shim that:
/// 1. Extracts platform providers from Cloudflare environment
/// 2. Parses incoming ActivityPub activities
/// 3. Verifies HTTP signatures using core
/// 4. Calls core.handle_inbox() for all business logic
///
/// All inbox processing logic (Follow, Undo, Create, Like, Announce, etc.)
/// is now in dais-core, making it reusable across platforms.

use worker::{self, event, Request, Response, Env, Context, Router, RouteContext, Result, Headers};
use dais_cloudflare::{D1Provider, WorkerHttpProvider};
use dais_core::{DaisCore, CoreConfig, CoreError};
use dais_core::activitypub::{HttpSignature, ContentModerator};
use std::collections::HashMap;
use async_trait::async_trait;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .options("/users/:username/inbox", |_req, _ctx| {
            let headers = Headers::new();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "POST, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Content-Type, Signature, Date, Digest")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(Response::empty()?.with_headers(headers))
        })
        .post_async("/users/:username/inbox", handle_inbox)
        .run(req, env)
        .await
}

async fn handle_inbox(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    worker::console_log!("Received activity for user: {}", username);

    // Get the request body as text
    let body = req.text().await?;
    worker::console_log!("Activity body (first 200 chars): {}", &body[..std::cmp::min(200, body.len())]);

    // Parse the activity to validate JSON
    let activity: serde_json::Value = match serde_json::from_str(&body) {
        Ok(a) => a,
        Err(e) => {
            worker::console_log!("Failed to parse activity JSON: {}", e);
            return Response::error("Invalid activity JSON", 400);
        }
    };

    // Verify HTTP signature
    let signature_header = match req.headers().get("Signature")? {
        Some(sig) => sig,
        None => {
            worker::console_log!("Missing Signature header");
            return Response::error("Missing signature", 401);
        }
    };

    worker::console_log!("Signature header: {}", signature_header);

    // Parse signature
    let http_signature = match HttpSignature::parse(&signature_header) {
        Ok(sig) => sig,
        Err(e) => {
            worker::console_log!("Failed to parse signature: {}", e);
            return Response::error("Invalid signature format", 400);
        }
    };

    // Build headers map for signature verification
    let mut headers_map = HashMap::new();
    if let Some(host) = req.headers().get("Host")? {
        headers_map.insert("host".to_string(), host);
    }
    if let Some(date) = req.headers().get("Date")? {
        headers_map.insert("date".to_string(), date);
    }
    if let Some(digest) = req.headers().get("Digest")? {
        headers_map.insert("digest".to_string(), digest);
    }
    if let Some(content_type) = req.headers().get("Content-Type")? {
        headers_map.insert("content-type".to_string(), content_type);
    }

    // Get request path
    let url = req.url()?;
    let path = url.path();

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);
    let http = WorkerHttpProvider::new();

    // Get configuration from environment variables
    let configured_domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = ctx.env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    let username_var = ctx.env.var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    // Get private key from secrets (for sending Accept/Reject responses)
    let private_key = ctx.env.secret("PRIVATE_KEY")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| {
            worker::console_log!("WARNING: PRIVATE_KEY secret not found");
            String::new()
        });

    // Note: Public key is stored in the database, not needed here
    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),
        username: username_var,
        private_key,
        public_key: "".to_string(),  // Not needed for inbox
        media_url: "".to_string(),    // Not needed for inbox
    };

    // Initialize DaisCore
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),  // Not used by inbox
        Box::new(PlaceholderQueue),    // Not used by inbox currently
        Box::new(http),
        config,
    );

    // Verify the HTTP signature and digest using core
    let actor_id = activity["actor"].as_str().unwrap_or("");
    worker::console_log!("Verifying signature from actor: {}", actor_id);

    // Verify digest if present
    if let Some(digest_header) = headers_map.get("digest") {
        if let Err(e) = dais_core::activitypub::verify_digest(&body, digest_header) {
            worker::console_log!("Digest verification failed: {}", e);
            return Response::error("Invalid digest", 400);
        }
        worker::console_log!("✓ Digest verified");
    }

    // Fetch actor's public key and verify signature
    let public_key = match dais_core::activitypub::fetch_actor_public_key(&*core.http(), actor_id).await {
        Ok(key) => key,
        Err(e) => {
            worker::console_log!("Failed to fetch actor public key: {}", e);
            return Response::error("Failed to verify signature", 401);
        }
    };

    let verified = match dais_core::activitypub::verify_request(
        &public_key,
        &http_signature,
        "POST",
        path,
        &headers_map,
    ) {
        Ok(v) => v,
        Err(e) => {
            worker::console_log!("Signature verification error: {}", e);
            return Response::error("Signature verification failed", 401);
        }
    };

    if !verified {
        worker::console_log!("✗ Signature verification failed");
        return Response::error("Invalid signature", 401);
    }

    worker::console_log!("✓ Signature verified successfully");

    // Build our actor URL
    let our_actor_url = format!("https://{}/users/{}", activitypub_domain, username);

    // Optional: Create content moderator
    // For now, we'll use None (no moderation)
    // In production, you could implement WorkersAIModerator using Cloudflare's AI
    let moderator: Option<&dyn ContentModerator> = None;

    // Call core logic - all inbox processing is in the core!
    match core.handle_inbox(body, our_actor_url, moderator).await {
        Ok(()) => {
            worker::console_log!("✓ Activity processed successfully");
            Response::empty()
        }
        Err(e) => {
            worker::console_log!("Error processing activity: {}", e);
            // Convert core errors to HTTP responses
            match e {
                CoreError::NotFound(msg) => Response::error(msg, 404),
                CoreError::InvalidActivity(msg) => Response::error(msg, 400),
                CoreError::Unauthorized(msg) => Response::error(msg, 401),
                _ => Response::error(format!("Internal error: {}", e), 500),
            }
        }
    }
}

// Placeholder providers for unused platform features
// These are only needed because DaisCore requires all providers,
// but inbox doesn't currently use storage/queue

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
