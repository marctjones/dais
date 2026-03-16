/// Refactored Outbox worker using dais-core
///
/// This is a thin shim that:
/// 1. Extracts platform providers from Cloudflare environment
/// 2. Handles content negotiation (JSON vs HTML)
/// 3. Calls core.get_outbox_posts() and core.get_post() for all business logic
///
/// All outbox query logic is now in dais-core, making it reusable across platforms.

use worker::{self, event, Request, Response, Env, Context, Router, RouteContext, Result, Headers};
use dais_cloudflare::D1Provider;
use dais_core::{DaisCore, CoreConfig, CoreError};
use dais_core::activitypub::Post;
use async_trait::async_trait;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/users/:username/outbox", handle_outbox)
        .get_async("/users/:username/posts/:post_id", handle_post)
        .run(req, env)
        .await
}

async fn handle_outbox(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    worker::console_log!("Fetching outbox for user: {}", username);

    // Check Accept header for content negotiation
    let accept = req.headers().get("Accept")?.unwrap_or_default();
    let wants_html = accept.contains("text/html");

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

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

    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),
        username: username_var,
        private_key: "".to_string(),
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    // Initialize DaisCore
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(PlaceholderHttp),
        config,
    );

    // Call core logic to get outbox posts
    let posts = match core.get_outbox_posts(username.to_string()).await {
        Ok(p) => p,
        Err(e) => {
            worker::console_log!("Error fetching outbox: {}", e);
            return match e {
                CoreError::NotFound(msg) => Response::error(msg, 404),
                _ => Response::error(format!("Internal error: {}", e), 500),
            };
        }
    };

    worker::console_log!("Found {} posts", posts.len());

    if wants_html {
        // Return HTML view (platform-specific)
        // TODO: Implement HTML rendering with theme support
        Response::error("HTML rendering not implemented yet", 501)
    } else {
        // Return ActivityPub JSON
        let outbox_json = build_outbox_collection(&activitypub_domain, username, &posts);

        let mut resp = Response::from_json(&outbox_json)?;
        resp.headers_mut().set("Content-Type", "application/activity+json")?;
        resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
        Ok(resp)
    }
}

async fn handle_post(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get username and post ID from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    let post_id = match ctx.param("post_id") {
        Some(id) => id,
        None => return Response::error("Post ID required", 400),
    };

    worker::console_log!("Fetching post: /users/{}/posts/{}", username, post_id);

    // Check Accept header for content negotiation
    let accept = req.headers().get("Accept")?.unwrap_or_default();
    let wants_html = accept.contains("text/html");

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

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

    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),
        username: username_var,
        private_key: "".to_string(),
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    // Initialize DaisCore
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(PlaceholderHttp),
        config,
    );

    // Call core logic to get the post
    let post = match core.get_post(username.to_string(), post_id.to_string()).await {
        Ok(p) => p,
        Err(e) => {
            worker::console_log!("Error fetching post: {}", e);
            return match e {
                CoreError::NotFound(msg) => Response::error(msg, 404),
                _ => Response::error(format!("Internal error: {}", e), 500),
            };
        }
    };

    if wants_html {
        // Return HTML view (platform-specific)
        // TODO: Implement HTML rendering with theme support and interactions
        Response::error("HTML rendering not implemented yet", 501)
    } else {
        // Return ActivityPub JSON
        let note_json = build_note_object(&post);

        let mut resp = Response::from_json(&note_json)?;
        resp.headers_mut().set("Content-Type", "application/activity+json")?;
        resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
        Ok(resp)
    }
}

/// Build ActivityPub OrderedCollection for outbox
fn build_outbox_collection(domain: &str, username: &str, posts: &[Post]) -> serde_json::Value {
    let outbox_url = format!("https://{}/users/{}/outbox", domain, username);

    let items: Vec<serde_json::Value> = posts.iter().map(|post| {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Create",
            "id": format!("{}#create", post.id),
            "actor": format!("https://{}/users/{}", domain, username),
            "published": post.published_at,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "cc": [format!("https://{}/users/{}/followers", domain, username)],
            "object": build_note_object(post)
        })
    }).collect();

    serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": outbox_url,
        "type": "OrderedCollection",
        "totalItems": items.len(),
        "orderedItems": items
    })
}

/// Build ActivityPub Note object from Post
fn build_note_object(post: &Post) -> serde_json::Value {
    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": post.id,
        "attributedTo": post.actor_id,
        "content": post.content,
        "published": post.published_at,
        "to": if post.visibility == "public" {
            vec!["https://www.w3.org/ns/activitystreams#Public"]
        } else {
            vec![]
        }
    });

    // Add optional fields
    if let Some(ref content_html) = post.content_html {
        note["contentMap"] = serde_json::json!({ "en": content_html });
    }

    if let Some(ref in_reply_to) = post.in_reply_to {
        note["inReplyTo"] = serde_json::json!(in_reply_to);
    }

    if let Some(ref attachments_json) = post.media_attachments {
        if let Ok(attachments) = serde_json::from_str::<serde_json::Value>(attachments_json) {
            note["attachment"] = attachments;
        }
    }

    note
}

// Placeholder providers for unused platform features

use dais_core::traits::{
    StorageProvider, QueueProvider, HttpProvider, PlatformResult, PlatformError,
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

struct PlaceholderHttp;

#[async_trait(?Send)]
impl HttpProvider for PlaceholderHttp {
    async fn fetch(&self, _request: dais_core::traits::Request) -> PlatformResult<dais_core::traits::Response> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }
}
