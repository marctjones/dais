/// Refactored PDS (Personal Data Server) worker for AT Protocol
///
/// This worker implements the AT Protocol endpoints for Bluesky compatibility.
///
/// NOTE: AT Protocol implementation is currently minimal. Full implementation
/// will be migrated to dais-core in a future update.
///
/// Endpoints:
/// - GET /xrpc/com.atproto.server.describeServer
/// - GET /xrpc/com.atproto.sync.getRepo
/// - GET /xrpc/app.bsky.feed.getAuthorFeed
/// - WebSocket /xrpc/com.atproto.sync.subscribeRepos

use worker::*;
use dais_cloudflare::D1Provider;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ServerDescription {
    #[serde(rename = "availableUserDomains")]
    available_user_domains: Vec<String>,
    #[serde(rename = "inviteCodeRequired")]
    invite_code_required: bool,
    links: Links,
}

#[derive(Serialize)]
struct Links {
    #[serde(rename = "privacyPolicy")]
    privacy_policy: Option<String>,
    #[serde(rename = "termsOfService")]
    terms_of_service: Option<String>,
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();

    console_log!("PDS: {} {}", req.method(), path);

    // Handle WebSocket upgrade for subscribeRepos
    if path == "/xrpc/com.atproto.sync.subscribeRepos" {
        // Check if this is a WebSocket upgrade request
        if let Some(upgrade) = req.headers().get("Upgrade")? {
            if upgrade.to_lowercase() == "websocket" {
                return handle_subscribe_repos(req, env).await;
            }
        }
        // Not a WebSocket request
        return Response::error("This endpoint requires WebSocket upgrade", 400);
    }

    let router = Router::new();

    router
        .get_async("/xrpc/com.atproto.server.describeServer", handle_describe_server)
        .get_async("/xrpc/com.atproto.sync.getRepo", handle_get_repo)
        .get_async("/xrpc/app.bsky.feed.getAuthorFeed", handle_get_author_feed)
        .get("/health", |_req, _ctx| Response::ok("PDS OK"))
        .run(req, env)
        .await
}

async fn handle_describe_server(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let domain = ctx.env.var("PDS_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "pds.dais.social".to_string());

    let description = ServerDescription {
        available_user_domains: vec![domain],
        invite_code_required: false,
        links: Links {
            privacy_policy: None,
            terms_of_service: None,
        },
    };

    Response::from_json(&description)
}

async fn handle_get_repo(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get DID from query parameter
    let url = req.url()?;
    let did = url.query_pairs()
        .find(|(key, _)| key == "did")
        .map(|(_, value)| value.to_string());

    if did.is_none() {
        return Response::error("Missing 'did' parameter", 400);
    }

    let did = did.unwrap();
    console_log!("Getting repo for DID: {}", did);

    // Get database
    let db = ctx.env.d1("DB")?;
    let db_provider = D1Provider::new(db);

    // Query for repo data
    // TODO: Implement full AT Protocol repo export
    // For now, return minimal response

    Response::from_json(&serde_json::json!({
        "did": did,
        "head": "bafy...", // TODO: Actual CID
        "rev": "1",
        "blocks": []
    }))
}

async fn handle_get_author_feed(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get actor from query parameter
    let url = req.url()?;
    let actor = url.query_pairs()
        .find(|(key, _)| key == "actor")
        .map(|(_, value)| value.to_string());

    if actor.is_none() {
        return Response::error("Missing 'actor' parameter", 400);
    }

    let actor = actor.unwrap();
    console_log!("Getting feed for actor: {}", actor);

    // Get database
    let db = ctx.env.d1("DB")?;
    let db_provider = D1Provider::new(db);

    // Query for posts
    // TODO: Implement full AT Protocol feed
    // For now, return empty feed

    Response::from_json(&serde_json::json!({
        "feed": []
    }))
}

async fn handle_subscribe_repos(req: Request, env: Env) -> Result<Response> {
    console_log!("WebSocket upgrade requested for subscribeRepos");

    // Accept WebSocket upgrade
    let pair = WebSocketPair::new()?;
    let server = pair.server;
    let client = pair.client;

    // Spawn a task to handle the WebSocket connection
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = handle_websocket(server, env).await {
            console_log!("WebSocket error: {:?}", e);
        }
    });

    // Return the client WebSocket to the browser
    Response::from_websocket(client)
}

async fn handle_websocket(mut ws: WebSocket, _env: Env) -> Result<()> {
    // Accept the WebSocket connection
    ws.accept()?;

    console_log!("WebSocket connection established for subscribeRepos");

    // Send initial message
    let info_msg = r##"{"t":"#info","info":{"name":"dais-pds","version":"1.1.0"}}"##;
    ws.send_with_str(info_msg)?;

    // Note: Full WebSocket event handling requires additional setup
    // For now, just keep the connection alive
    // TODO: Implement full AT Protocol firehose when needed

    Ok(())
}
