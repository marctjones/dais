use serde::Serialize;
use serde_json::Value;
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
/// - GET /xrpc/com.atproto.sync.getRepoStatus
/// - GET /xrpc/com.atproto.sync.listRepos
/// - GET /xrpc/com.atproto.repo.describeRepo
/// - GET /xrpc/com.atproto.repo.getRecord
/// - GET /xrpc/app.bsky.feed.getAuthorFeed
/// - WebSocket /xrpc/com.atproto.sync.subscribeRepos
use worker::*;

#[derive(Serialize)]
struct ServerDescription {
    did: String,
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

#[durable_object]
pub struct RelaySubscription {
    _state: State,
    _env: Env,
}

impl DurableObject for RelaySubscription {
    fn new(state: State, env: Env) -> Self {
        Self {
            _state: state,
            _env: env,
        }
    }

    async fn fetch(&self, _req: Request) -> Result<Response> {
        Response::error(
            "AT Protocol relay subscription Durable Object is not active",
            501,
        )
    }
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
        return json_response(serde_json::json!({
            "endpoint": "com.atproto.sync.subscribeRepos",
            "transport": "websocket",
            "status": "available",
            "message": "Use a WebSocket Upgrade request to subscribe to repo events"
        }));
    }

    let router = Router::new();

    router
        .get_async(
            "/xrpc/com.atproto.server.describeServer",
            handle_describe_server,
        )
        .get_async("/.well-known/did.json", handle_did_document)
        .get_async("/xrpc/com.atproto.sync.getRepo", handle_get_repo)
        .get_async("/xrpc/com.atproto.sync.getRepoStatus", handle_get_repo_status)
        .get_async("/xrpc/com.atproto.sync.listRepos", handle_list_repos)
        .get_async("/xrpc/com.atproto.repo.describeRepo", handle_describe_repo)
        .get_async("/xrpc/com.atproto.repo.getRecord", handle_get_record)
        .get_async("/xrpc/app.bsky.feed.getAuthorFeed", handle_get_author_feed)
        .get("/health", |_req, _ctx| Response::ok("PDS OK"))
        .run(req, env)
        .await
}

async fn handle_describe_server(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = identity(&ctx.env);

    let description = ServerDescription {
        did: identity.did,
        available_user_domains: vec![identity.handle],
        invite_code_required: false,
        links: Links {
            privacy_policy: None,
            terms_of_service: None,
        },
    };

    Response::from_json(&description)
}

async fn handle_did_document(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = identity(&ctx.env);
    json_response(serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/suites/secp256k1-2019/v1"
        ],
        "id": identity.did,
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": format!("https://{}", identity.pds_hostname)
        }]
    }))
}

async fn handle_get_repo(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let did = required_query(&url, "did")?;
    console_log!("Getting repo for DID: {}", did);
    let stats = repo_stats(&ctx.env).await?;
    json_response(serde_json::json!({
        "did": did,
        "head": stats.head,
        "rev": stats.rev,
        "records": stats.records,
        "blocks": [],
        "warning": "dais exposes a JSON compatibility floor here; full CAR/MST repo export is not implemented"
    }))
}

async fn handle_get_repo_status(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let did = required_query(&url, "did")?;
    let stats = repo_stats(&ctx.env).await?;
    json_response(serde_json::json!({
        "did": did,
        "active": true,
        "status": "active",
        "rev": stats.rev,
        "head": stats.head
    }))
}

async fn handle_list_repos(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let identity = identity(&ctx.env);
    let stats = repo_stats(&ctx.env).await?;
    json_response(serde_json::json!({
        "repos": [{
            "did": identity.did,
            "head": stats.head,
            "rev": stats.rev,
            "active": true,
            "status": "active"
        }]
    }))
}

async fn handle_describe_repo(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let repo = required_query(&url, "repo")?;
    let identity = identity(&ctx.env);
    if repo != identity.did && repo != identity.handle {
        return Response::error("Repo not found", 404);
    }
    json_response(serde_json::json!({
        "handle": identity.handle,
        "did": identity.did,
        "didDoc": {
            "id": identity.did,
            "service": [{
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": format!("https://{}", identity.pds_hostname)
            }]
        },
        "collections": ["app.bsky.feed.post"],
        "handleIsCorrect": true
    }))
}

async fn handle_get_record(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let repo = required_query(&url, "repo")?;
    let collection = required_query(&url, "collection")?;
    let rkey = required_query(&url, "rkey")?;
    let identity = identity(&ctx.env);
    if repo != identity.did && repo != identity.handle {
        return Response::error("Repo not found", 404);
    }
    if collection != "app.bsky.feed.post" {
        return Response::error("Collection not found", 404);
    }

    let Some(row) = find_public_post(&ctx.env, &rkey).await? else {
        return Response::error("Record not found", 404);
    };
    json_response(record_response(&identity, row))
}

async fn handle_get_author_feed(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    console_log!("Getting feed for actor: {}", actor);
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(serde_json::json!({ "feed": [] }));
    }
    let rows = public_posts(&ctx.env, query_limit(&url)).await?;
    let feed: Vec<Value> = rows
        .into_iter()
        .map(|row| serde_json::json!({ "post": post_view(&identity, row) }))
        .collect();
    json_response(serde_json::json!({ "feed": feed }))
}

async fn handle_subscribe_repos(_req: Request, env: Env) -> Result<Response> {
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

async fn handle_websocket(ws: WebSocket, _env: Env) -> Result<()> {
    // Accept the WebSocket connection
    ws.accept()?;

    console_log!("WebSocket connection established for subscribeRepos");

    // Send initial message
    let info_msg = r##"{"t":"#info","info":{"name":"dais-pds","version":"1.1.0"}}"##;
    ws.send_with_str(info_msg)?;

    // The compatibility floor announces availability; commit streaming is tracked in GitHub issues.

    Ok(())
}

#[derive(Clone)]
struct Identity {
    did: String,
    handle: String,
    pds_hostname: String,
}

struct RepoStats {
    head: String,
    rev: String,
    records: usize,
}

fn identity(env: &Env) -> Identity {
    let handle = env
        .var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());
    let pds_hostname = env
        .var("PDS_HOSTNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "pds.dais.social".to_string());
    Identity {
        did: format!("did:web:{handle}"),
        handle,
        pds_hostname,
    }
}

fn required_query(url: &Url, key: &str) -> Result<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| worker::Error::RustError(format!("Missing '{key}' parameter")))
}

fn query_limit(url: &Url) -> u32 {
    url.query_pairs()
        .find(|(name, _)| name == "limit")
        .and_then(|(_, value)| value.parse::<u32>().ok())
        .unwrap_or(30)
        .clamp(1, 100)
}

async fn repo_stats(env: &Env) -> Result<RepoStats> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT COUNT(*) AS records, MAX(COALESCE(updated_at, published_at)) AS rev
            FROM posts
            WHERE visibility = 'public'
              AND encrypted_message IS NULL
              AND content NOT LIKE '%End-to-end encrypted message%'
            "#,
        )
        .first::<serde_json::Map<String, Value>>(None)
        .await?
        .unwrap_or_default();
    let records = row.get("records").and_then(Value::as_u64).unwrap_or(0) as usize;
    let rev = row
        .get("rev")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .to_string();
    Ok(RepoStats {
        head: stable_cid(&rev),
        rev,
        records,
    })
}

async fn public_posts(env: &Env, limit: u32) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid
        FROM posts
        WHERE visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind(&[limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn find_public_post(
    env: &Env,
    rkey: &str,
) -> Result<Option<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    let uri_suffix = format!("/{rkey}");
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid
        FROM posts
        WHERE visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
          AND (
            id = ?1
            OR id LIKE ?2
            OR atproto_uri = ?1
            OR atproto_uri LIKE ?2
          )
        ORDER BY published_at DESC
        LIMIT 1
        "#,
    )
    .bind(&[rkey.into(), format!("%{uri_suffix}").into()])?
    .first::<serde_json::Map<String, Value>>(None)
    .await
}

fn record_response(identity: &Identity, row: serde_json::Map<String, Value>) -> Value {
    let uri = at_uri(identity, &row);
    let cid = row
        .get("atproto_cid")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_cid(&uri));
    serde_json::json!({
        "uri": uri,
        "cid": cid,
        "value": record_value(row)
    })
}

fn post_view(identity: &Identity, row: serde_json::Map<String, Value>) -> Value {
    let uri = at_uri(identity, &row);
    let cid = row
        .get("atproto_cid")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| stable_cid(&uri));
    serde_json::json!({
        "uri": uri,
        "cid": cid,
        "author": {
            "did": identity.did,
            "handle": identity.handle,
            "displayName": "dais"
        },
        "record": record_value(row.clone()),
        "replyCount": 0,
        "repostCount": 0,
        "likeCount": 0,
        "indexedAt": row.get("published_at").and_then(Value::as_str).unwrap_or("")
    })
}

fn record_value(row: serde_json::Map<String, Value>) -> Value {
    serde_json::json!({
        "$type": "app.bsky.feed.post",
        "text": row.get("content").and_then(Value::as_str).unwrap_or(""),
        "createdAt": row.get("published_at").and_then(Value::as_str).unwrap_or("")
    })
}

fn at_uri(identity: &Identity, row: &serde_json::Map<String, Value>) -> String {
    row.get("atproto_uri")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            let id = row.get("id").and_then(Value::as_str).unwrap_or("");
            let rkey = id.rsplit('/').next().unwrap_or(id);
            format!("at://{}/app.bsky.feed.post/{rkey}", identity.did)
        })
}

fn stable_cid(value: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("bafy{:016x}", hasher.finish())
}

fn json_response(value: Value) -> Result<Response> {
    let mut response = Response::from_json(&value)?;
    response.headers_mut().set("Content-Type", "application/json")?;
    Ok(response)
}
