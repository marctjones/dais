use serde::{Deserialize, Serialize};
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
/// - GET /xrpc/com.atproto.sync.getBlob
/// - GET /xrpc/com.atproto.sync.getRepoStatus
/// - GET /xrpc/com.atproto.sync.listRepos
/// - GET /xrpc/com.atproto.repo.describeRepo
/// - GET /xrpc/com.atproto.repo.getRecord
/// - GET /xrpc/app.bsky.actor.getProfile
/// - GET /xrpc/app.bsky.actor.getProfiles
/// - GET /xrpc/app.bsky.feed.getAuthorFeed
/// - GET /xrpc/app.bsky.feed.getTimeline
/// - GET /xrpc/app.bsky.feed.searchPosts
/// - GET /xrpc/app.bsky.actor.searchActors
/// - GET /xrpc/app.bsky.actor.searchActorsTypeahead
/// - GET /xrpc/app.bsky.notification.listNotifications
/// - GET /xrpc/app.bsky.feed.getLikes
/// - GET /xrpc/app.bsky.graph.getFollowers
/// - GET /xrpc/app.bsky.graph.getFollows
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
        .get_async("/xrpc/com.atproto.sync.getBlob", handle_get_blob)
        .get_async(
            "/xrpc/com.atproto.sync.getRepoStatus",
            handle_get_repo_status,
        )
        .get_async("/xrpc/com.atproto.sync.listRepos", handle_list_repos)
        .get_async("/xrpc/com.atproto.repo.describeRepo", handle_describe_repo)
        .get_async("/xrpc/com.atproto.repo.getRecord", handle_get_record)
        .get_async("/xrpc/app.bsky.actor.getProfile", handle_get_profile)
        .get_async("/xrpc/app.bsky.actor.getProfiles", handle_get_profiles)
        .get_async("/xrpc/app.bsky.feed.getAuthorFeed", handle_get_author_feed)
        .get_async("/xrpc/app.bsky.feed.getTimeline", handle_get_timeline)
        .get_async("/xrpc/app.bsky.feed.searchPosts", handle_search_posts)
        .get_async("/xrpc/app.bsky.actor.searchActors", handle_search_actors)
        .get_async(
            "/xrpc/app.bsky.actor.searchActorsTypeahead",
            handle_search_actors,
        )
        .get_async(
            "/xrpc/app.bsky.notification.listNotifications",
            handle_list_notifications,
        )
        .get_async("/xrpc/app.bsky.feed.getLikes", handle_get_likes)
        .get_async("/xrpc/app.bsky.graph.getFollowers", handle_get_followers)
        .get_async("/xrpc/app.bsky.graph.getFollows", handle_get_follows)
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

async fn handle_get_blob(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let did = required_query(&url, "did")?;
    let cid = required_query(&url, "cid")?;
    let identity = identity(&ctx.env);
    if did != identity.did && did != identity.handle {
        return Response::error("Blob not found", 404);
    }

    let Some(blob) = public_media_by_cid(&ctx.env, &cid).await? else {
        return Response::error("Blob not found", 404);
    };
    let bucket = ctx.env.bucket("MEDIA_BUCKET")?;
    let Some(object) = bucket.get(blob.key).execute().await? else {
        return Response::error("Blob not found", 404);
    };
    let content_type = object
        .http_metadata()
        .content_type
        .unwrap_or(blob.media_type);
    let Some(body) = object.body() else {
        return Response::error("Blob has no body", 404);
    };
    let mut response = Response::from_bytes(body.bytes().await?)?;
    response
        .headers_mut()
        .set("Content-Type", content_type.as_str())?;
    response
        .headers_mut()
        .set("Cache-Control", "public, max-age=31536000, immutable")?;
    Ok(response)
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

async fn handle_get_profile(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(profile_view(&identity, &actor, "", ""));
    }
    json_response(local_profile_view(&ctx.env, &identity).await?)
}

async fn handle_get_profiles(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let identity = identity(&ctx.env);
    let mut profiles = Vec::new();
    for (_, actor) in url.query_pairs().filter(|(name, _)| name == "actors") {
        if actor == identity.did || actor == identity.handle {
            profiles.push(local_profile_view(&ctx.env, &identity).await?);
        } else {
            profiles.push(profile_view(&identity, actor.as_ref(), "", ""));
        }
    }
    json_response(serde_json::json!({ "profiles": profiles }))
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

async fn handle_get_timeline(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let identity = identity(&ctx.env);
    let rows = public_posts(&ctx.env, query_limit(&url)).await?;
    let feed: Vec<Value> = rows
        .into_iter()
        .map(|row| serde_json::json!({ "post": post_view(&identity, row) }))
        .collect();
    json_response(serde_json::json!({ "feed": feed }))
}

async fn handle_search_posts(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let query = required_query(&url, "q")?;
    let identity = identity(&ctx.env);
    if query.trim().is_empty() {
        return json_response(serde_json::json!({ "posts": [] }));
    }
    let posts: Vec<Value> = search_public_posts(&ctx.env, &query, query_limit(&url))
        .await?
        .into_iter()
        .map(|row| post_view(&identity, row))
        .collect();
    json_response(serde_json::json!({ "posts": posts }))
}

async fn handle_search_actors(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let query = required_query(&url, "q")?.to_ascii_lowercase();
    let identity = identity(&ctx.env);
    let mut actors = Vec::new();
    if query.trim().is_empty()
        || identity.handle.to_ascii_lowercase().contains(&query)
        || identity.did.to_ascii_lowercase().contains(&query)
        || "dais".contains(&query)
    {
        actors.push(local_profile_view(&ctx.env, &identity).await?);
    }
    json_response(serde_json::json!({ "actors": actors }))
}

async fn handle_list_notifications(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let identity = identity(&ctx.env);
    let rows = notification_rows(&ctx.env, query_limit(&url)).await?;
    let notifications: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            let indexed_at = string_field(&row, "created_at");
            let activity_id = string_field(&row, "activity_id");
            let uri = if activity_id.is_empty() {
                string_field(&row, "id")
            } else {
                activity_id
            };
            serde_json::json!({
                "uri": uri,
                "cid": stable_cid(&format!("{}{}", actor_id, indexed_at)),
                "author": profile_view(
                    &identity,
                    &actor_id,
                    &string_field(&row, "actor_username"),
                    &string_field(&row, "actor_display_name"),
                ),
                "reason": string_field(&row, "kind"),
                "reasonSubject": string_field(&row, "post_id"),
                "record": {
                    "$type": "app.bsky.notification.listNotifications#notification",
                    "text": string_field(&row, "content"),
                    "createdAt": indexed_at
                },
                "isRead": bool_field(&row, "read"),
                "indexedAt": indexed_at
            })
        })
        .collect();
    json_response(serde_json::json!({ "notifications": notifications }))
}

async fn handle_get_likes(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let uri = required_query(&url, "uri")?;
    let identity = identity(&ctx.env);
    let rows = like_rows(&ctx.env, &uri, query_limit(&url)).await?;
    let likes: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            let created_at = string_field(&row, "created_at");
            serde_json::json!({
                "actor": profile_view(
                    &identity,
                    &actor_id,
                    &string_field(&row, "actor_username"),
                    &string_field(&row, "actor_display_name"),
                ),
                "createdAt": created_at,
                "indexedAt": created_at
            })
        })
        .collect();
    json_response(serde_json::json!({
        "uri": uri,
        "cid": stable_cid(&uri),
        "likes": likes
    }))
}

async fn handle_get_followers(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(serde_json::json!({ "followers": [] }));
    }
    let followers: Vec<Value> = follower_rows(&ctx.env, query_limit(&url))
        .await?
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            profile_view(&identity, &actor_id, "", "")
        })
        .collect();
    json_response(serde_json::json!({ "followers": followers }))
}

async fn handle_get_follows(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(serde_json::json!({ "follows": [] }));
    }
    let follows: Vec<Value> = follows_rows(&ctx.env, query_limit(&url))
        .await?
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            profile_view(&identity, &actor_id, "", "")
        })
        .collect();
    json_response(serde_json::json!({ "follows": follows }))
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

struct ProfileCounts {
    posts: u64,
    followers: u64,
    follows: u64,
}

#[derive(Clone, Deserialize)]
struct MediaAttachment {
    url: String,
    #[serde(default, rename = "mediaType")]
    media_type: String,
    #[serde(default)]
    name: String,
}

struct PublicMediaBlob {
    key: String,
    media_type: String,
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

async fn profile_counts(env: &Env) -> Result<ProfileCounts> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT
              (SELECT COUNT(*) FROM posts
               WHERE visibility = 'public'
                 AND encrypted_message IS NULL
                 AND content NOT LIKE '%End-to-end encrypted message%') AS posts,
              (SELECT COUNT(*) FROM followers WHERE status = 'approved') AS followers,
              (SELECT COUNT(*) FROM following WHERE status = 'accepted') AS follows
            "#,
        )
        .first::<serde_json::Map<String, Value>>(None)
        .await?
        .unwrap_or_default();
    Ok(ProfileCounts {
        posts: row.get("posts").and_then(Value::as_u64).unwrap_or(0),
        followers: row.get("followers").and_then(Value::as_u64).unwrap_or(0),
        follows: row.get("follows").and_then(Value::as_u64).unwrap_or(0),
    })
}

async fn public_posts(env: &Env, limit: u32) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid, media_attachments
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

async fn search_public_posts(
    env: &Env,
    query: &str,
    limit: u32,
) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid, media_attachments
        FROM posts
        WHERE visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
          AND instr(LOWER(content), LOWER(?1)) > 0
        ORDER BY published_at DESC
        LIMIT ?2
        "#,
    )
    .bind(&[query.trim().into(), limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn notification_rows(env: &Env, limit: u32) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, type AS kind, actor_id, actor_username, actor_display_name,
               actor_avatar_url, post_id, activity_id, content, read, created_at
        FROM notifications
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(&[limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn like_rows(
    env: &Env,
    uri: &str,
    limit: u32,
) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, actor_id, actor_username, actor_display_name,
               actor_avatar_url, object_url, created_at
        FROM interactions
        WHERE type = 'like'
          AND object_url = ?1
        ORDER BY created_at DESC
        LIMIT ?2
        "#,
    )
    .bind(&[uri.into(), limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn follower_rows(env: &Env, limit: u32) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT follower_actor_id AS actor_id, created_at
        FROM followers
        WHERE status = 'approved'
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(&[limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn follows_rows(env: &Env, limit: u32) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT target_actor_id AS actor_id, created_at
        FROM following
        WHERE status = 'accepted'
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(&[limit.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn find_public_post(env: &Env, rkey: &str) -> Result<Option<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    let uri_suffix = format!("/{rkey}");
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid, media_attachments
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

async fn public_media_by_cid(env: &Env, cid: &str) -> Result<Option<PublicMediaBlob>> {
    let db = env.d1("DB")?;
    let rows = db
        .prepare(
            r#"
            SELECT media_attachments
            FROM posts
            WHERE visibility = 'public'
              AND encrypted_message IS NULL
              AND content NOT LIKE '%End-to-end encrypted message%'
              AND media_attachments IS NOT NULL
              AND media_attachments != ''
            ORDER BY published_at DESC
            LIMIT 200
            "#,
        )
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;

    for row in rows {
        for attachment in media_attachments(&row) {
            if stable_cid(&attachment.url) == cid {
                let Some(key) = r2_key_from_media_url(&attachment.url) else {
                    continue;
                };
                if !attachment.media_type.starts_with("image/") {
                    continue;
                }
                return Ok(Some(PublicMediaBlob {
                    key,
                    media_type: attachment.media_type,
                }));
            }
        }
    }

    Ok(None)
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

fn profile_view(identity: &Identity, actor_id: &str, handle: &str, display_name: &str) -> Value {
    if actor_id == identity.did || actor_id == identity.handle || actor_id.is_empty() {
        return serde_json::json!({
            "did": identity.did,
            "handle": identity.handle,
            "displayName": "dais"
        });
    }

    let handle = if handle.is_empty() {
        actor_handle(actor_id)
    } else {
        handle.to_string()
    };
    let display_name = if display_name.is_empty() {
        handle.clone()
    } else {
        display_name.to_string()
    };

    serde_json::json!({
        "did": actor_id,
        "handle": handle,
        "displayName": display_name
    })
}

async fn local_profile_view(env: &Env, identity: &Identity) -> Result<Value> {
    let counts = profile_counts(env).await?;
    Ok(serde_json::json!({
        "did": identity.did,
        "handle": identity.handle,
        "displayName": "dais",
        "description": "Private-by-default social server.",
        "followersCount": counts.followers,
        "followsCount": counts.follows,
        "postsCount": counts.posts,
        "indexedAt": "1970-01-01T00:00:00Z"
    }))
}

fn actor_handle(actor_id: &str) -> String {
    if let Ok(url) = Url::parse(actor_id) {
        let username = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or("")
            .trim_start_matches('@');
        if let Some(host) = url.host_str() {
            if !username.is_empty() {
                return format!("{username}.{host}");
            }
            return host.to_string();
        }
    }
    actor_id.to_string()
}

fn record_value(row: serde_json::Map<String, Value>) -> Value {
    let mut record = serde_json::json!({
        "$type": "app.bsky.feed.post",
        "text": row.get("content").and_then(Value::as_str).unwrap_or(""),
        "createdAt": row.get("published_at").and_then(Value::as_str).unwrap_or("")
    });
    let images: Vec<Value> = media_attachments(&row)
        .into_iter()
        .filter(|attachment| attachment.media_type.starts_with("image/"))
        .map(|attachment| {
            serde_json::json!({
                "alt": attachment.name,
                "image": {
                    "$type": "blob",
                    "ref": { "$link": stable_cid(&attachment.url) },
                    "mimeType": attachment.media_type,
                    "size": 0
                }
            })
        })
        .collect();
    if !images.is_empty() {
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "embed".to_string(),
                serde_json::json!({
                    "$type": "app.bsky.embed.images",
                    "images": images
                }),
            );
        }
    }
    record
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

fn media_attachments(row: &serde_json::Map<String, Value>) -> Vec<MediaAttachment> {
    let raw = row
        .get("media_attachments")
        .and_then(Value::as_str)
        .unwrap_or("");
    serde_json::from_str::<Vec<MediaAttachment>>(raw).unwrap_or_default()
}

fn r2_key_from_media_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "https" || parsed.host_str()? != "social.dais.social" {
        return None;
    }
    let path = parsed.path();
    let key = path.strip_prefix("/media/")?;
    if key.contains("_private") || key.contains("../") || !key.starts_with("uploads/") {
        return None;
    }
    Some(key.to_string())
}

fn json_response(value: Value) -> Result<Response> {
    let mut response = Response::from_json(&value)?;
    response
        .headers_mut()
        .set("Content-Type", "application/json")?;
    Ok(response)
}

fn string_field(row: &serde_json::Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn bool_field(row: &serde_json::Map<String, Value>, key: &str) -> bool {
    row.get(key).and_then(Value::as_bool).unwrap_or_else(|| {
        row.get(key)
            .and_then(Value::as_u64)
            .map(|value| value != 0)
            .unwrap_or(false)
    })
}
