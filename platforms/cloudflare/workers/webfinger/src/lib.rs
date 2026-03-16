/// Refactored WebFinger worker using dais-core
///
/// This is a thin shim that:
/// 1. Extracts platform providers from Cloudflare environment
/// 2. Initializes DaisCore with configuration
/// 3. Calls core.webfinger() for all logic
///
/// Compare to the original workers/webfinger/src/lib.rs - this is much simpler!

use worker::{self, event, Request, Response, Env, Context, Router, RouteContext, Result};
use dais_cloudflare::D1Provider;
use dais_core::{DaisCore, CoreConfig};

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/.well-known/webfinger", handle_webfinger)
        .run(req, env)
        .await
}

async fn handle_webfinger(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Parse query parameters
    let url = req.url()?;
    let resource = url
        .query_pairs()
        .find(|(key, _)| key == "resource")
        .map(|(_, value)| value.to_string());

    let resource = match resource {
        Some(r) => r,
        None => {
            return Response::error("Missing 'resource' query parameter", 400);
        }
    };

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

    // Get configuration from environment variables
    let configured_domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = ctx.env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    let username = ctx.env.var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    // Create core config (minimal for webfinger - doesn't need keys/PDS)
    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),  // Not needed for webfinger
        username,
        private_key: "".to_string(),  // Not needed for webfinger
        public_key: "".to_string(),   // Not needed for webfinger
        media_url: "".to_string(),    // Not needed for webfinger
    };

    // Initialize DaisCore (with placeholder providers for unused features)
    // For webfinger, we only need the database
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),  // Not used by webfinger
        Box::new(PlaceholderQueue),    // Not used by webfinger
        Box::new(PlaceholderHttp),     // Not used by webfinger
        config,
    );

    // Call core logic - all business logic is in the core!
    let response = match core.webfinger(resource).await {
        Ok(resp) => resp,
        Err(e) => {
            // Convert core errors to HTTP responses
            return match e {
                dais_core::CoreError::NotFound(msg) => Response::error(msg, 404),
                dais_core::CoreError::InvalidActivity(msg) => Response::error(msg, 400),
                _ => Response::error(format!("Internal error: {}", e), 500),
            };
        }
    };

    // Build HTTP response
    let mut resp = Response::from_json(&response)?;
    resp.headers_mut().set("Content-Type", "application/jrd+json")?;
    resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
    resp.headers_mut().set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    resp.headers_mut().set("Access-Control-Allow-Headers", "Content-Type")?;
    Ok(resp)
}

// Placeholder providers for unused platform features
// These are only needed because DaisCore requires all providers,
// but webfinger doesn't use storage/queue/http

use dais_core::traits::*;
use async_trait::async_trait;

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

    async fn list_detailed(&self, _options: dais_core::traits::ListOptions) -> PlatformResult<dais_core::traits::ListResult> {
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

struct PlaceholderHttp;

#[async_trait(?Send)]
impl HttpProvider for PlaceholderHttp {
    async fn fetch(&self, _request: dais_core::traits::Request) -> PlatformResult<dais_core::traits::Response> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }
}
