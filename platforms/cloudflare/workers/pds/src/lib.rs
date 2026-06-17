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
/// - POST /xrpc/com.atproto.server.createSession
/// - POST /xrpc/com.atproto.repo.uploadBlob
/// - GET /xrpc/com.atproto.sync.getRepo
/// - GET /xrpc/com.atproto.sync.getBlob
/// - GET /xrpc/com.atproto.sync.getRepoStatus
/// - GET /xrpc/com.atproto.sync.listRepos
/// - GET /xrpc/com.atproto.repo.describeRepo
/// - GET /xrpc/com.atproto.repo.getRecord
/// - GET /xrpc/com.atproto.repo.listRecords
/// - POST /xrpc/com.atproto.repo.createRecord
/// - POST /xrpc/com.atproto.repo.deleteRecord
/// - GET /xrpc/app.bsky.actor.getProfile
/// - GET /xrpc/app.bsky.actor.getProfiles
/// - GET /xrpc/app.bsky.feed.getAuthorFeed
/// - GET /xrpc/app.bsky.feed.getTimeline
/// - GET /xrpc/app.bsky.feed.searchPosts
/// - GET /xrpc/app.bsky.actor.searchActors
/// - GET /xrpc/app.bsky.actor.searchActorsTypeahead
/// - GET /xrpc/app.bsky.actor.getPreferences
/// - GET /xrpc/app.bsky.notification.listNotifications
/// - GET /xrpc/app.bsky.feed.getLikes
/// - GET /xrpc/app.bsky.graph.getFollowers
/// - GET /xrpc/app.bsky.graph.getFollows
/// - GET /xrpc/app.bsky.graph.getBlocks
/// - GET /xrpc/app.bsky.graph.getMutes
/// - GET /xrpc/app.bsky.labeler.getServices
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
        .post_async(
            "/xrpc/com.atproto.server.createSession",
            handle_create_session,
        )
        .post_async("/xrpc/com.atproto.repo.uploadBlob", handle_upload_blob)
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
        .get_async("/xrpc/com.atproto.repo.listRecords", handle_list_records)
        .post_async("/xrpc/com.atproto.repo.createRecord", handle_create_record)
        .post_async("/xrpc/com.atproto.repo.deleteRecord", handle_delete_record)
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
            "/xrpc/app.bsky.actor.getPreferences",
            handle_get_preferences,
        )
        .get_async(
            "/xrpc/app.bsky.notification.listNotifications",
            handle_list_notifications,
        )
        .get_async("/xrpc/app.bsky.feed.getLikes", handle_get_likes)
        .get_async("/xrpc/app.bsky.graph.getFollowers", handle_get_followers)
        .get_async("/xrpc/app.bsky.graph.getFollows", handle_get_follows)
        .get_async("/xrpc/app.bsky.graph.getBlocks", handle_get_blocks)
        .get_async("/xrpc/app.bsky.graph.getMutes", handle_get_mutes)
        .get_async(
            "/xrpc/app.bsky.labeler.getServices",
            handle_get_labeler_services,
        )
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

async fn handle_create_session(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: CreateSessionRequest = req.json().await?;
    let identity = identity(&ctx.env);
    if body.identifier != identity.did && body.identifier != identity.handle {
        return Response::error("Account not found", 401);
    }
    let owner_token = owner_api_token(&ctx.env)?;
    if body.password != owner_token {
        return Response::error("Invalid identifier or password", 401);
    }
    json_response(serde_json::json!({
        "accessJwt": owner_token,
        "refreshJwt": stable_cid(&format!("{}:refresh", identity.did)),
        "handle": identity.handle,
        "did": identity.did,
        "active": true
    }))
}

async fn handle_upload_blob(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    let content_type = req
        .headers()
        .get("Content-Type")?
        .and_then(|value| {
            value
                .split(';')
                .next()
                .map(str::trim)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !content_type.starts_with("image/") {
        return Response::error("Only public image blobs are supported", 400);
    }
    let bytes = req.bytes().await?;
    if bytes.is_empty() {
        return Response::error("Blob body is required", 400);
    }
    let size = bytes.len() as u64;
    let cid = stable_cid(&format!(
        "{}:{}",
        content_type,
        bytes.iter().fold(0u64, |acc, byte| acc
            .wrapping_mul(31)
            .wrapping_add(*byte as u64))
    ));
    let ext = extension_for_media_type(&content_type);
    let key = format!("uploads/atproto/{cid}.{ext}");
    ctx.env
        .bucket("MEDIA_BUCKET")?
        .put(key, bytes)
        .execute()
        .await?;
    json_response(serde_json::json!({
        "blob": {
            "$type": "blob",
            "ref": { "$link": cid },
            "mimeType": content_type,
            "size": size
        }
    }))
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
    let identity = identity(&ctx.env);
    let stats = repo_stats(&ctx.env, &identity).await?;
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
    let identity = identity(&ctx.env);
    let stats = repo_stats(&ctx.env, &identity).await?;
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
    let stats = repo_stats(&ctx.env, &identity).await?;
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
        "collections": [
            "app.bsky.actor.profile",
            "app.bsky.feed.post",
            "app.bsky.feed.like",
            "app.bsky.feed.repost",
            "app.bsky.graph.follow"
        ],
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
    if collection == "app.bsky.actor.profile" {
        if rkey != "self" {
            return Response::error("Record not found", 404);
        }
        return json_response(profile_record_response(&ctx.env, &identity).await?);
    }
    if collection != "app.bsky.feed.post" {
        return Response::error("Collection not found", 404);
    }

    let Some(row) = find_public_post(&ctx.env, &rkey).await? else {
        return Response::error("Record not found", 404);
    };
    json_response(record_response(&identity, row))
}

async fn handle_list_records(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    let url = req.url()?;
    let repo = required_query(&url, "repo")?;
    let collection = required_query(&url, "collection")?;
    let identity = identity(&ctx.env);
    if repo != identity.did && repo != identity.handle {
        return Response::error("Repo not found", 404);
    }
    let page = query_page(&url);
    let records = match collection.as_str() {
        "app.bsky.feed.post" => public_posts(&ctx.env, page)
            .await?
            .into_iter()
            .map(|row| record_response(&identity, row))
            .collect(),
        "app.bsky.feed.like" => {
            subject_records(&ctx.env, &identity, &collection, "like", page).await?
        }
        "app.bsky.feed.repost" => {
            subject_records(&ctx.env, &identity, &collection, "boost", page).await?
        }
        "app.bsky.graph.follow" => follow_records(&ctx.env, &identity, page).await?,
        "app.bsky.actor.profile" => {
            vec![profile_record_response(&ctx.env, &identity).await?]
        }
        _ => return Response::error("Collection not found", 404),
    };
    paged_array_response("records", records, page)
}

async fn handle_create_record(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    let body: CreateRecordRequest = req.json().await?;
    let identity = identity(&ctx.env);
    if body.repo != identity.did && body.repo != identity.handle {
        return Response::error("Repo not found", 404);
    }
    let record_type = body
        .record
        .get("$type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if record_type != body.collection {
        return Response::error("Record type mismatch", 400);
    }
    if body.collection == "app.bsky.feed.like" || body.collection == "app.bsky.feed.repost" {
        return create_subject_record(&ctx.env, &identity, body).await;
    }
    if body.collection == "app.bsky.graph.follow" {
        return create_follow_record(&ctx.env, &identity, body).await;
    }
    if body.collection == "app.bsky.actor.profile" {
        return create_profile_record(&ctx.env, &identity, body).await;
    }
    if body.collection != "app.bsky.feed.post" {
        return Response::error(
            "Collection not writable in dais PDS compatibility mode",
            400,
        );
    }
    let text = body
        .record
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return Response::error("Post text is required", 400);
    }

    let created_at = body
        .record
        .get("createdAt")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default()
        });
    let rkey = body.rkey.unwrap_or_else(|| {
        format!(
            "{}-{}",
            created_at
                .chars()
                .filter(|c| c.is_ascii_digit())
                .take(14)
                .collect::<String>(),
            stable_cid(&format!("{}\n{}", created_at, text))
                .chars()
                .skip(4)
                .take(8)
                .collect::<String>()
        )
    });
    let actor_id = local_actor_id(&identity);
    let post_id = format!("{actor_id}/posts/{rkey}");
    let atproto_uri = format!("at://{}/app.bsky.feed.post/{rkey}", identity.did);
    let record_json = serde_json::to_string(&body.record)
        .map_err(|error| worker::Error::RustError(error.to_string()))?;
    let cid = stable_cid(&record_json);
    let content_html = format!("<p>{}</p>", html_escape(text).replace('\n', "<br>"));
    let media_attachments = atproto_media_attachments(&body.record)?;
    let media_attachments_json = if media_attachments.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&media_attachments)
            .map_err(|error| worker::Error::RustError(error.to_string()))?
    };
    let in_reply_to = body
        .record
        .get("reply")
        .and_then(|reply| reply.get("parent"))
        .and_then(|parent| parent.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let atproto_reply_json = body
        .record
        .get("reply")
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| worker::Error::RustError(error.to_string()))?
        .unwrap_or_default();

    ctx.env
        .d1("DB")?
        .prepare(
            r#"
            INSERT INTO posts (
              id, actor_id, content, content_html, object_type, visibility, protocol,
              published_at, in_reply_to, atproto_uri, atproto_cid, media_attachments,
              atproto_reply_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, 'Note', 'public', 'atproto', ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&[
            post_id.clone().into(),
            actor_id.into(),
            text.into(),
            content_html.into(),
            created_at.into(),
            in_reply_to.into(),
            atproto_uri.clone().into(),
            cid.clone().into(),
            media_attachments_json.into(),
            atproto_reply_json.into(),
        ])?
        .run()
        .await?;

    json_response(serde_json::json!({
        "uri": atproto_uri,
        "cid": cid,
        "commit": {
            "cid": stable_cid(&format!("commit:{}", post_id)),
            "rev": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default()
        }
    }))
}

async fn handle_delete_record(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    let body: DeleteRecordRequest = req.json().await?;
    let identity = identity(&ctx.env);
    if body.repo != identity.did && body.repo != identity.handle {
        return Response::error("Repo not found", 404);
    }
    if body.collection == "app.bsky.feed.like" || body.collection == "app.bsky.feed.repost" {
        let atproto_uri = record_uri(&identity, &body.collection, &body.rkey);
        let id_suffix = format!("/{}", body.rkey);
        ctx.env
            .d1("DB")?
            .prepare("DELETE FROM interactions WHERE id = ?1 OR id LIKE ?2")
            .bind(&[atproto_uri.into(), format!("%{id_suffix}").into()])?
            .run()
            .await?;
        return delete_record_response(&body.rkey);
    }
    if body.collection == "app.bsky.graph.follow" {
        let atproto_uri = record_uri(&identity, &body.collection, &body.rkey);
        let id_suffix = format!("/{}", body.rkey);
        ctx.env
            .d1("DB")?
            .prepare("DELETE FROM following WHERE id = ?1 OR id LIKE ?2")
            .bind(&[atproto_uri.into(), format!("%{id_suffix}").into()])?
            .run()
            .await?;
        return delete_record_response(&body.rkey);
    }
    if body.collection == "app.bsky.actor.profile" {
        if body.rkey != "self" {
            return Response::error("Record not found", 404);
        }
        ctx.env
            .d1("DB")?
            .prepare(
                r#"
                UPDATE actors
                SET display_name = NULL,
                    summary = NULL,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = ?1 OR username = 'social'
                "#,
            )
            .bind(&[local_actor_id(&identity).into()])?
            .run()
            .await?;
        return delete_record_response(&body.rkey);
    }
    if body.collection != "app.bsky.feed.post" {
        return Response::error(
            "Collection not writable in dais PDS compatibility mode",
            400,
        );
    }
    let atproto_uri = format!("at://{}/app.bsky.feed.post/{}", identity.did, body.rkey);
    let id_suffix = format!("/{}", body.rkey);
    ctx.env
        .d1("DB")?
        .prepare(
            r#"
            DELETE FROM posts
            WHERE visibility = 'public'
              AND encrypted_message IS NULL
              AND (atproto_uri = ?1 OR id LIKE ?2)
            "#,
        )
        .bind(&[atproto_uri.into(), format!("%{id_suffix}").into()])?
        .run()
        .await?;
    delete_record_response(&body.rkey)
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
    let page = query_page(&url);
    let rows = public_posts(&ctx.env, page).await?;
    let feed: Vec<Value> = rows
        .into_iter()
        .map(|row| serde_json::json!({ "post": post_view(&identity, row) }))
        .collect();
    paged_array_response("feed", feed, page)
}

async fn handle_get_timeline(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let identity = identity(&ctx.env);
    let page = query_page(&url);
    let rows = public_posts(&ctx.env, page).await?;
    let feed: Vec<Value> = rows
        .into_iter()
        .map(|row| serde_json::json!({ "post": post_view(&identity, row) }))
        .collect();
    paged_array_response("feed", feed, page)
}

async fn handle_search_posts(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let query = required_query(&url, "q")?;
    let identity = identity(&ctx.env);
    if query.trim().is_empty() {
        return json_response(serde_json::json!({ "posts": [] }));
    }
    let page = query_page(&url);
    let posts: Vec<Value> = search_public_posts(&ctx.env, &query, page)
        .await?
        .into_iter()
        .map(|row| post_view(&identity, row))
        .collect();
    paged_array_response("posts", posts, page)
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

async fn handle_get_preferences(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    json_response(serde_json::json!({
        "preferences": [
            {
                "$type": "app.bsky.actor.defs#adultContentPref",
                "enabled": false
            },
            {
                "$type": "app.bsky.actor.defs#savedFeedsPref",
                "pinned": [],
                "saved": []
            },
            {
                "$type": "app.bsky.actor.defs#threadViewPref",
                "sort": "oldest",
                "prioritizeFollowedUsers": false
            },
            {
                "$type": "app.bsky.actor.defs#mutedWordsPref",
                "items": []
            },
            {
                "$type": "app.bsky.actor.defs#hiddenPostsPref",
                "items": []
            },
            {
                "$type": "app.bsky.actor.defs#labelersPref",
                "labelers": []
            }
        ]
    }))
}

async fn handle_list_notifications(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let identity = identity(&ctx.env);
    let page = query_page(&url);
    let rows = notification_rows(&ctx.env, page).await?;
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
    paged_array_response("notifications", notifications, page)
}

async fn handle_get_likes(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let uri = required_query(&url, "uri")?;
    let identity = identity(&ctx.env);
    let page = query_page(&url);
    let rows = like_rows(&ctx.env, &uri, page).await?;
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
    let cursor = next_cursor(page, likes.len());
    let mut likes = likes;
    likes.truncate(page.limit as usize);
    let mut body = serde_json::json!({
        "uri": uri,
        "cid": stable_cid(&uri),
        "likes": likes
    });
    if let (Some(object), Some(cursor)) = (body.as_object_mut(), cursor) {
        object.insert("cursor".to_string(), Value::String(cursor));
    }
    json_response(body)
}

async fn handle_get_followers(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(serde_json::json!({ "followers": [] }));
    }
    let page = query_page(&url);
    let followers: Vec<Value> = follower_rows(&ctx.env, page)
        .await?
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            profile_view(&identity, &actor_id, "", "")
        })
        .collect();
    paged_array_response("followers", followers, page)
}

async fn handle_get_follows(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let actor = required_query(&url, "actor")?;
    let identity = identity(&ctx.env);
    if actor != identity.did && actor != identity.handle {
        return json_response(serde_json::json!({ "follows": [] }));
    }
    let page = query_page(&url);
    let follows: Vec<Value> = follows_rows(&ctx.env, page)
        .await?
        .into_iter()
        .map(|row| {
            let actor_id = string_field(&row, "actor_id");
            profile_view(&identity, &actor_id, "", "")
        })
        .collect();
    paged_array_response("follows", follows, page)
}

async fn handle_get_blocks(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    json_response(serde_json::json!({ "blocks": [] }))
}

async fn handle_get_mutes(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if !owner_bearer_matches(&req, &ctx.env)? {
        return Response::error("Unauthorized", 401);
    }
    json_response(serde_json::json!({ "mutes": [] }))
}

async fn handle_get_labeler_services(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    json_response(serde_json::json!({ "views": [] }))
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

    let identity = identity(&_env);
    let stats = repo_stats(&_env, &identity).await?;
    let posts = public_posts(
        &_env,
        Page {
            limit: 100,
            offset: 0,
        },
    )
    .await?;
    let mut ops = vec![serde_json::json!({
        "action": "update",
        "path": "app.bsky.actor.profile/self",
        "cid": { "$link": stable_cid(&format!("profile:{}", stats.rev)) }
    })];
    for row in posts.into_iter().take(99) {
        let uri = at_uri(&identity, &row);
        let rkey = uri.rsplit('/').next().unwrap_or("");
        if rkey.is_empty() {
            continue;
        }
        let cid = row
            .get("atproto_cid")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| stable_cid(&uri));
        ops.push(serde_json::json!({
            "action": "update",
            "path": format!("app.bsky.feed.post/{rkey}"),
            "cid": { "$link": cid }
        }));
    }
    let commit_msg = serde_json::json!({
        "t": "#commit",
        "commit": {
            "seq": repo_seq(&stats.rev),
            "rebase": false,
            "tooBig": false,
            "repo": identity.did,
            "commit": { "$link": stats.head },
            "rev": stats.rev,
            "since": null,
            "blocks": "",
            "ops": ops,
            "blobs": [],
            "time": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default()
        }
    });
    ws.send_with_str(&commit_msg.to_string())?;

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

struct ActorProfile {
    display_name: String,
    description: String,
}

#[derive(Clone, Copy)]
struct Page {
    limit: u32,
    offset: u32,
}

#[derive(Clone, Deserialize, Serialize)]
struct MediaAttachment {
    #[serde(default = "default_image_attachment_type", rename = "type")]
    attachment_type: String,
    url: String,
    #[serde(default, rename = "mediaType")]
    media_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    cid: String,
    #[serde(default)]
    size: u64,
}

fn default_image_attachment_type() -> String {
    "Image".to_string()
}

struct PublicMediaBlob {
    key: String,
    media_type: String,
}

#[derive(Deserialize)]
struct CreateSessionRequest {
    identifier: String,
    password: String,
}

#[derive(Deserialize)]
struct CreateRecordRequest {
    repo: String,
    collection: String,
    record: Value,
    rkey: Option<String>,
}

#[derive(Deserialize)]
struct DeleteRecordRequest {
    repo: String,
    collection: String,
    rkey: String,
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

fn owner_api_token(env: &Env) -> Result<String> {
    env.secret("OWNER_API_TOKEN")
        .map(|secret| secret.to_string())
        .or_else(|_| env.var("OWNER_API_TOKEN").map(|var| var.to_string()))
        .map_err(|_| worker::Error::RustError("OWNER_API_TOKEN is not configured".to_string()))
}

fn owner_bearer_matches(req: &Request, env: &Env) -> Result<bool> {
    let expected = owner_api_token(env)?;
    let header = req.headers().get("Authorization")?.unwrap_or_default();
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Ok(false);
    };
    Ok(token == expected)
}

fn query_limit(url: &Url) -> u32 {
    url.query_pairs()
        .find(|(name, _)| name == "limit")
        .and_then(|(_, value)| value.parse::<u32>().ok())
        .unwrap_or(30)
        .clamp(1, 100)
}

fn query_page(url: &Url) -> Page {
    Page {
        limit: query_limit(url),
        offset: url
            .query_pairs()
            .find(|(name, _)| name == "cursor")
            .and_then(|(_, value)| parse_cursor_offset(&value))
            .unwrap_or(0),
    }
}

fn parse_cursor_offset(value: &str) -> Option<u32> {
    value
        .strip_prefix("offset:")
        .unwrap_or(value)
        .parse::<u32>()
        .ok()
}

fn next_cursor(page: Page, row_count: usize) -> Option<String> {
    (row_count > page.limit as usize).then(|| (page.offset + page.limit).to_string())
}

fn page_size(page: Page) -> u32 {
    page.limit + 1
}

fn paged_array_response(key: &str, mut values: Vec<Value>, page: Page) -> Result<Response> {
    let cursor = next_cursor(page, values.len());
    values.truncate(page.limit as usize);
    let mut object = serde_json::Map::new();
    object.insert(key.to_string(), Value::Array(values));
    if let Some(cursor) = cursor {
        object.insert("cursor".to_string(), Value::String(cursor));
    }
    json_response(Value::Object(object))
}

async fn repo_stats(env: &Env, identity: &Identity) -> Result<RepoStats> {
    let db = env.d1("DB")?;
    let actor_id = local_actor_id(identity);
    let row = db
        .prepare(
            r#"
            SELECT COUNT(*) AS records, MAX(rev) AS rev
            FROM (
              SELECT COALESCE(updated_at, created_at) AS rev
              FROM actors
              WHERE id = ?1
              UNION ALL
              SELECT COALESCE(updated_at, published_at) AS rev
              FROM posts
              WHERE visibility = 'public'
                AND encrypted_message IS NULL
                AND content NOT LIKE '%End-to-end encrypted message%'
              UNION ALL
              SELECT created_at AS rev
              FROM interactions
              WHERE actor_id = ?1
                AND type IN ('like', 'boost')
              UNION ALL
              SELECT COALESCE(accepted_at, created_at) AS rev
              FROM following
              WHERE actor_id = ?1
                AND status IN ('accepted', 'pending')
            )
            "#,
        )
        .bind(&[actor_id.into()])?
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

async fn public_posts(env: &Env, page: Page) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid, media_attachments, atproto_reply_json
        FROM posts
        WHERE visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
        ORDER BY published_at DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )
    .bind(&[page_size(page).into(), page.offset.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn search_public_posts(
    env: &Env,
    query: &str,
    page: Page,
) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, content, published_at, COALESCE(updated_at, published_at) AS updated_at,
               atproto_uri, atproto_cid, media_attachments, atproto_reply_json
        FROM posts
        WHERE visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
          AND instr(LOWER(content), LOWER(?1)) > 0
        ORDER BY published_at DESC
        LIMIT ?2 OFFSET ?3
        "#,
    )
    .bind(&[
        query.trim().into(),
        page_size(page).into(),
        page.offset.into(),
    ])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn notification_rows(env: &Env, page: Page) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, type AS kind, actor_id, actor_username, actor_display_name,
               actor_avatar_url, post_id, activity_id, content, read, created_at
        FROM notifications
        ORDER BY created_at DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )
    .bind(&[page_size(page).into(), page.offset.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn like_rows(
    env: &Env,
    uri: &str,
    page: Page,
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
        LIMIT ?2 OFFSET ?3
        "#,
    )
    .bind(&[uri.into(), page_size(page).into(), page.offset.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn follower_rows(env: &Env, page: Page) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT follower_actor_id AS actor_id, created_at
        FROM followers
        WHERE status = 'approved'
        ORDER BY created_at DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )
    .bind(&[page_size(page).into(), page.offset.into()])?
    .all()
    .await?
    .results::<serde_json::Map<String, Value>>()
}

async fn follows_rows(env: &Env, page: Page) -> Result<Vec<serde_json::Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT target_actor_id AS actor_id, created_at
        FROM following
        WHERE status = 'accepted'
        ORDER BY created_at DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )
    .bind(&[page_size(page).into(), page.offset.into()])?
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
               atproto_uri, atproto_cid, media_attachments, atproto_reply_json
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
            if media_attachment_cid(&attachment) == cid {
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

async fn create_subject_record(
    env: &Env,
    identity: &Identity,
    body: CreateRecordRequest,
) -> Result<Response> {
    let subject = body
        .record
        .get("subject")
        .and_then(Value::as_object)
        .ok_or_else(|| worker::Error::RustError("subject is required".to_string()))?;
    let subject_uri = subject
        .get("uri")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| worker::Error::RustError("subject.uri is required".to_string()))?;
    let created_at = record_created_at(&body.record);
    let collection = body.collection;
    let interaction_type = if collection == "app.bsky.feed.like" {
        "like"
    } else {
        "boost"
    };
    let rkey = body
        .rkey
        .unwrap_or_else(|| generated_rkey(&created_at, subject_uri));
    let uri = record_uri(identity, &collection, &rkey);
    env.d1("DB")?
        .prepare(
            r#"
            INSERT OR REPLACE INTO interactions (
              id, type, actor_id, object_url, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&[
            uri.clone().into(),
            interaction_type.into(),
            local_actor_id(identity).into(),
            subject_uri.into(),
            created_at.into(),
        ])?
        .run()
        .await?;
    create_record_response(&uri, &body.record)
}

async fn create_follow_record(
    env: &Env,
    identity: &Identity,
    body: CreateRecordRequest,
) -> Result<Response> {
    let subject_did = body
        .record
        .get("subject")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| worker::Error::RustError("subject DID is required".to_string()))?;
    if subject_did == identity.did {
        return Response::error("Cannot follow the local DID", 400);
    }
    let created_at = record_created_at(&body.record);
    let rkey = body
        .rkey
        .unwrap_or_else(|| generated_rkey(&created_at, subject_did));
    let uri = record_uri(identity, "app.bsky.graph.follow", &rkey);
    env.d1("DB")?
        .prepare(
            r#"
            INSERT INTO following (
              id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
            ) VALUES (?1, ?2, ?3, '', 'accepted', ?4, ?4)
            ON CONFLICT(actor_id, target_actor_id) DO UPDATE SET
              id = excluded.id,
              status = 'accepted',
              created_at = excluded.created_at,
              accepted_at = excluded.accepted_at
            "#,
        )
        .bind(&[
            uri.clone().into(),
            local_actor_id(identity).into(),
            subject_did.into(),
            created_at.into(),
        ])?
        .run()
        .await?;
    create_record_response(&uri, &body.record)
}

async fn create_profile_record(
    env: &Env,
    identity: &Identity,
    body: CreateRecordRequest,
) -> Result<Response> {
    let rkey = body.rkey.unwrap_or_else(|| "self".to_string());
    if rkey != "self" {
        return Response::error("Profile record key must be self", 400);
    }
    if body.record.get("avatar").is_some() || body.record.get("banner").is_some() {
        return Response::error("Profile avatar/banner blobs are not supported yet", 400);
    }
    let display_name = body
        .record
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let description = body
        .record
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    env.d1("DB")?
        .prepare(
            r#"
            UPDATE actors
            SET display_name = ?1,
                summary = ?2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?3 OR username = 'social'
            "#,
        )
        .bind(&[
            display_name.into(),
            description.into(),
            local_actor_id(identity).into(),
        ])?
        .run()
        .await?;
    let uri = record_uri(identity, "app.bsky.actor.profile", "self");
    create_record_response(&uri, &body.record)
}

async fn subject_records(
    env: &Env,
    identity: &Identity,
    collection: &str,
    interaction_type: &str,
    page: Page,
) -> Result<Vec<Value>> {
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT id, object_url, created_at
            FROM interactions
            WHERE actor_id = ?1
              AND type = ?2
            ORDER BY created_at DESC
            LIMIT ?3 OFFSET ?4
            "#,
        )
        .bind(&[
            local_actor_id(identity).into(),
            interaction_type.into(),
            page_size(page).into(),
            page.offset.into(),
        ])?
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let subject_uri = string_field(&row, "object_url");
            let uri = record_uri_from_row(identity, collection, &row);
            serde_json::json!({
                "uri": uri,
                "cid": stable_cid(&format!("{}:{}", collection, subject_uri)),
                "value": {
                    "$type": collection,
                    "subject": {
                        "uri": subject_uri,
                        "cid": stable_cid(&subject_uri)
                    },
                    "createdAt": string_field(&row, "created_at")
                }
            })
        })
        .collect())
}

async fn follow_records(env: &Env, identity: &Identity, page: Page) -> Result<Vec<Value>> {
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT id, target_actor_id, created_at
            FROM following
            WHERE actor_id = ?1
              AND status IN ('accepted', 'pending')
            ORDER BY created_at DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )
        .bind(&[
            local_actor_id(identity).into(),
            page_size(page).into(),
            page.offset.into(),
        ])?
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let subject = string_field(&row, "target_actor_id");
            let uri = record_uri_from_row(identity, "app.bsky.graph.follow", &row);
            serde_json::json!({
                "uri": uri,
                "cid": stable_cid(&format!("app.bsky.graph.follow:{subject}")),
                "value": {
                    "$type": "app.bsky.graph.follow",
                    "subject": subject,
                    "createdAt": string_field(&row, "created_at")
                }
            })
        })
        .collect())
}

async fn profile_record_response(env: &Env, identity: &Identity) -> Result<Value> {
    let profile = actor_profile(env, identity).await?;
    let value = profile_record_value(&profile);
    Ok(serde_json::json!({
        "uri": record_uri(identity, "app.bsky.actor.profile", "self"),
        "cid": stable_cid(&serde_json::to_string(&value).unwrap_or_default()),
        "value": value
    }))
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

fn create_record_response(uri: &str, record: &Value) -> Result<Response> {
    json_response(serde_json::json!({
        "uri": uri,
        "cid": stable_cid(&serde_json::to_string(record).unwrap_or_default()),
        "commit": {
            "cid": stable_cid(&format!("commit:{uri}")),
            "rev": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default()
        }
    }))
}

fn delete_record_response(rkey: &str) -> Result<Response> {
    json_response(serde_json::json!({
        "commit": {
            "cid": stable_cid(&format!("delete:{rkey}")),
            "rev": js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_default()
        }
    }))
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
    let profile = actor_profile(env, identity).await?;
    Ok(serde_json::json!({
        "did": identity.did,
        "handle": identity.handle,
        "displayName": profile.display_name,
        "description": profile.description,
        "followersCount": counts.followers,
        "followsCount": counts.follows,
        "postsCount": counts.posts,
        "indexedAt": "1970-01-01T00:00:00Z"
    }))
}

async fn actor_profile(env: &Env, identity: &Identity) -> Result<ActorProfile> {
    let row = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT display_name, summary
            FROM actors
            WHERE id = ?1 OR username = 'social'
            LIMIT 1
            "#,
        )
        .bind(&[local_actor_id(identity).into()])?
        .first::<serde_json::Map<String, Value>>(None)
        .await?
        .unwrap_or_default();
    let display_name = row
        .get("display_name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("dais")
        .to_string();
    let description = row
        .get("summary")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Private-by-default social server.")
        .to_string();
    Ok(ActorProfile {
        display_name,
        description,
    })
}

fn profile_record_value(profile: &ActorProfile) -> Value {
    let mut value = serde_json::json!({
        "$type": "app.bsky.actor.profile"
    });
    if let Some(object) = value.as_object_mut() {
        if !profile.display_name.is_empty() {
            object.insert(
                "displayName".to_string(),
                Value::String(profile.display_name.clone()),
            );
        }
        if !profile.description.is_empty() {
            object.insert(
                "description".to_string(),
                Value::String(profile.description.clone()),
            );
        }
    }
    value
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
    if let Some(reply) = row
        .get("atproto_reply_json")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
    {
        if let Some(object) = record.as_object_mut() {
            object.insert("reply".to_string(), reply);
        }
    } else if let Some(in_reply_to) = row
        .get("in_reply_to")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("at://"))
    {
        let cid = stable_cid(in_reply_to);
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "reply".to_string(),
                serde_json::json!({
                    "root": {
                        "uri": in_reply_to,
                        "cid": cid
                    },
                    "parent": {
                        "uri": in_reply_to,
                        "cid": cid
                    }
                }),
            );
        }
    }
    let images: Vec<Value> = media_attachments(&row)
        .into_iter()
        .filter(|attachment| attachment.media_type.starts_with("image/"))
        .map(|attachment| {
            let cid = media_attachment_cid(&attachment);
            serde_json::json!({
                "alt": attachment.name,
                "image": {
                    "$type": "blob",
                    "ref": { "$link": cid },
                    "mimeType": attachment.media_type,
                    "size": attachment.size
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

fn local_actor_id(identity: &Identity) -> String {
    format!("https://{}/users/social", identity.handle)
}

fn record_created_at(record: &Value) -> String {
    record
        .get("createdAt")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default()
        })
}

fn generated_rkey(created_at: &str, seed: &str) -> String {
    format!(
        "{}-{}",
        created_at
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(14)
            .collect::<String>(),
        stable_cid(&format!("{created_at}\n{seed}"))
            .chars()
            .skip(4)
            .take(8)
            .collect::<String>()
    )
}

fn record_uri(identity: &Identity, collection: &str, rkey: &str) -> String {
    format!("at://{}/{collection}/{rkey}", identity.did)
}

fn record_uri_from_row(
    identity: &Identity,
    collection: &str,
    row: &serde_json::Map<String, Value>,
) -> String {
    let id = string_field(row, "id");
    if id.starts_with("at://") {
        return id;
    }
    let rkey = id.rsplit('/').next().unwrap_or(id.as_str());
    record_uri(identity, collection, rkey)
}

fn stable_cid(value: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("bafy{:016x}", hasher.finish())
}

fn repo_seq(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish().max(1)
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn media_attachments(row: &serde_json::Map<String, Value>) -> Vec<MediaAttachment> {
    let raw = row
        .get("media_attachments")
        .and_then(Value::as_str)
        .unwrap_or("");
    serde_json::from_str::<Vec<MediaAttachment>>(raw).unwrap_or_default()
}

fn atproto_media_attachments(record: &Value) -> Result<Vec<MediaAttachment>> {
    let Some(embed) = record.get("embed") else {
        return Ok(Vec::new());
    };
    if embed.get("$type").and_then(Value::as_str) != Some("app.bsky.embed.images") {
        return Err(worker::Error::RustError(
            "Only image embeds are supported in dais PDS compatibility mode".to_string(),
        ));
    }
    let images = embed
        .get("images")
        .and_then(Value::as_array)
        .ok_or_else(|| worker::Error::RustError("embed.images must be an array".to_string()))?;
    let mut attachments = Vec::new();
    for image in images.iter().take(4) {
        let blob = image
            .get("image")
            .ok_or_else(|| worker::Error::RustError("image blob is required".to_string()))?;
        let cid = blob
            .get("ref")
            .and_then(|ref_value| ref_value.get("$link"))
            .and_then(Value::as_str)
            .ok_or_else(|| worker::Error::RustError("image.ref.$link is required".to_string()))?;
        let media_type = blob
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("image/png");
        if !media_type.starts_with("image/") {
            return Err(worker::Error::RustError(
                "Only image embeds are supported".to_string(),
            ));
        }
        let ext = extension_for_media_type(media_type);
        attachments.push(MediaAttachment {
            attachment_type: "Image".to_string(),
            url: format!("https://social.dais.social/media/uploads/atproto/{cid}.{ext}"),
            media_type: media_type.to_string(),
            name: image
                .get("alt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            cid: cid.to_string(),
            size: blob.get("size").and_then(Value::as_u64).unwrap_or_default(),
        });
    }
    Ok(attachments)
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

fn media_attachment_cid(attachment: &MediaAttachment) -> String {
    if attachment.cid.is_empty() {
        stable_cid(&attachment.url)
    } else {
        attachment.cid.clone()
    }
}

fn extension_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
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
