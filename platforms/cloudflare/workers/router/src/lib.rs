use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dais_core::activitypub::sign_request;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::{JsCast, JsValue};
use worker::{
    event, Context, D1Type, Env, Fetch, FormData, FormEntry, Headers, Request, RequestInit,
    Response, Result, ScheduleContext, ScheduledEvent,
};

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";
const SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "activitypub",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const REFRESHABLE_SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const WATCH_SOURCE_TYPES: &[&str] = &[
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const DEFAULT_ACTIVITYPUB_SEARCH_SERVERS: &[&str] =
    &["mastodon.social", "mstdn.social", "fosstodon.org"];
const MAX_ACTIVITYPUB_SEARCH_SERVERS: usize = 5;
const BLUESKY_APPVIEW_BASE_URL: &str = "https://api.bsky.app";
const TOOTFINDER_SEARCH_BASE_URL: &str = "https://www.tootfinder.ch/rest/api/search";

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();
    let host = url.host_str().unwrap_or_default();

    if host == activitypub_domain(&env) && path == "/" {
        let target = url.join(&format!("/users/{}", local_username(&env)))?;
        return Response::redirect(target);
    }

    if path.starts_with("/api/dais/owner/") {
        return handle_owner_api(req, env, &url).await;
    }

    if path.starts_with("/api/v1/") || path.starts_with("/api/v2/") || path.starts_with("/oauth/") {
        return handle_mastodon_api(req, env, &url).await;
    }

    if path.starts_with("/media/") {
        return handle_media(req, env, &url).await;
    }

    if path == "/.well-known/webfinger" && req.method() == worker::Method::Get {
        return activitypub_webfinger(&env, &url);
    }
    if activitypub_public_path(&env, path) {
        return handle_activitypub_public(req, env, &url).await;
    }

    match path {
        "/__dais-fixtures/activitypub/actor" => fixture_actor_response(&url),
        "/__dais-fixtures/activitypub/outbox" => fixture_outbox_response(&url),
        "/__dais-fixtures/activitypub/posts/public-preview" => fixture_post_response(&url),
        "/__dais-fixtures/sources/rss" => fixture_rss_response(&url),
        "/.well-known/oauth-authorization-server" if req.method() == worker::Method::Get => {
            oauth_authorization_server_metadata(&url)
        }
        "/.well-known/openid-configuration" if req.method() == worker::Method::Get => {
            oauth_authorization_server_metadata(&url)
        }
        "/.well-known/nodeinfo" if req.method() == worker::Method::Get => nodeinfo_discovery(&url),
        "/nodeinfo/2.0" if req.method() == worker::Method::Get => {
            api_json(&nodeinfo_document(&env).await?, 200)
        }
        "/health" => Response::ok("OK"),
        _ => Response::error(
            "Rust router migration scaffold: route not migrated yet",
            501,
        ),
    }
}

#[event(scheduled)]
async fn scheduled(_event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    console_error_panic_hook::set_once();
    ctx.wait_until(async move {
        let _ = refresh_due_sources(&env).await;
    });
}

fn env_string(env: &Env, name: &str, fallback: &str) -> String {
    env.var(name)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| fallback.to_string())
}

fn local_username(env: &Env) -> String {
    env_string(env, "USERNAME", "social")
}

fn handle_domain(env: &Env) -> String {
    env_string(env, "DOMAIN", "dais.social")
}

fn activitypub_domain(env: &Env) -> String {
    env.var("ACTIVITYPUB_DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| format!("social.{}", handle_domain(env)))
}

fn local_actor_url(env: &Env) -> String {
    format!(
        "https://{}/users/{}",
        activitypub_domain(env),
        local_username(env)
    )
}

fn local_actor_url_for_request(env: &Env, url: &worker::Url) -> String {
    format!("{}/users/{}", origin(url), local_username(env))
}

fn activitypub_user_prefix(env: &Env) -> String {
    format!("/users/{}", local_username(env))
}

async fn handle_mastodon_api(mut req: Request, env: Env, url: &worker::Url) -> Result<Response> {
    if req.method() == worker::Method::Options {
        return api_json(&serde_json::json!({}), 204);
    }

    let path = url.path();
    match (req.method(), path) {
        (worker::Method::Get, "/api/v1/instance") | (worker::Method::Get, "/api/v2/instance") => {
            api_json(
                &mastodon_instance(&env, path == "/api/v2/instance").await?,
                200,
            )
        }
        (worker::Method::Post, "/api/v1/apps") => {
            let body = read_mastodon_body(&mut req).await;
            api_json(&mastodon_create_app(&body), 200)
        }
        (worker::Method::Get, "/oauth/authorize") => oauth_authorize(url),
        (worker::Method::Post, "/oauth/token") => {
            let body = read_mastodon_body(&mut req).await;
            mastodon_oauth_token(&body)
        }
        (worker::Method::Post, "/oauth/revoke") => api_json(&serde_json::json!({}), 200),
        (worker::Method::Get, "/api/v1/accounts/verify_credentials") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&mastodon_account(&env).await?, 200)
        }
        (worker::Method::Patch, "/api/v1/accounts/update_credentials") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let body = read_mastodon_body(&mut req).await;
            mastodon_update_credentials(&env, &body).await
        }
        (worker::Method::Get, "/api/v1/accounts/relationships") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&mastodon_relationships(&env, url).await?, 200)
        }
        (worker::Method::Get, "/api/v1/preferences") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&mastodon_preferences(&env).await?, 200)
        }
        (worker::Method::Get, "/api/v1/custom_emojis")
        | (worker::Method::Get, "/api/v1/announcements")
        | (worker::Method::Get, "/api/v1/directory")
        | (worker::Method::Get, "/api/v1/trends")
        | (worker::Method::Get, "/api/v1/trends/statuses")
        | (worker::Method::Get, "/api/v1/trends/tags")
        | (worker::Method::Get, "/api/v1/trends/links") => api_json(&Vec::<Value>::new(), 200),
        (worker::Method::Get, "/api/v1/markers") | (worker::Method::Post, "/api/v1/markers") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&serde_json::json!({}), 200)
        }
        (worker::Method::Get, "/api/v1/follow_requests")
        | (worker::Method::Get, "/api/v1/suggestions")
        | (worker::Method::Get, "/api/v1/endorsements")
        | (worker::Method::Get, "/api/v1/featured_tags")
        | (worker::Method::Get, "/api/v1/followed_tags")
        | (worker::Method::Get, "/api/v1/scheduled_statuses")
        | (worker::Method::Get, "/api/v1/mutes")
        | (worker::Method::Get, "/api/v1/bookmarks")
        | (worker::Method::Get, "/api/v1/filters")
        | (worker::Method::Get, "/api/v2/filters")
        | (worker::Method::Get, "/api/v1/lists") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&Vec::<Value>::new(), 200)
        }
        (worker::Method::Post, _) if mastodon_follow_request_action(path) => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&serde_json::json!({}), 200)
        }
        (worker::Method::Get, "/api/v1/timelines/public") => api_json(
            &mastodon_statuses(&env, clamp_limit(query_param(url, "limit")), url).await?,
            200,
        ),
        (worker::Method::Get, "/api/v1/timelines/home") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_statuses(&env, clamp_limit(query_param(url, "limit")), url).await?,
                200,
            )
        }
        (worker::Method::Get, "/api/v1/favourites") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_statuses_by_interaction(
                    &env,
                    "like",
                    clamp_limit(query_param(url, "limit")),
                    url,
                )
                .await?,
                200,
            )
        }
        (worker::Method::Get, "/api/v1/blocks") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_blocks(&env, clamp_limit(query_param(url, "limit"))).await?,
                200,
            )
        }
        (worker::Method::Get, "/api/v1/domain_blocks") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_domain_blocks(&env, clamp_limit(query_param(url, "limit"))).await?,
                200,
            )
        }
        (worker::Method::Post, "/api/v1/domain_blocks") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let body = read_mastodon_body(&mut req).await;
            mastodon_set_domain_block(&env, &body, url, true).await
        }
        (worker::Method::Delete, "/api/v1/domain_blocks") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let body = read_mastodon_body(&mut req).await;
            mastodon_set_domain_block(&env, &body, url, false).await
        }
        (worker::Method::Get, "/api/v1/conversations") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_conversations(&env, clamp_limit(query_param(url, "limit"))).await?,
                200,
            )
        }
        (worker::Method::Get, "/api/v1/search") | (worker::Method::Get, "/api/v2/search") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_search(
                    &env,
                    &query_param(url, "q").unwrap_or_default(),
                    clamp_limit(query_param(url, "limit")),
                    url,
                )
                .await?,
                200,
            )
        }
        (worker::Method::Get, "/api/v1/notifications") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_notifications(&env, clamp_limit(query_param(url, "limit"))).await?,
                200,
            )
        }
        (worker::Method::Get, _) if mastodon_account_statuses_path(path) => api_json(
            &mastodon_statuses(&env, clamp_limit(query_param(url, "limit")), url).await?,
            200,
        ),
        (worker::Method::Get, _) if mastodon_account_followers_path(path) => api_json(
            &mastodon_followers(&env, clamp_limit(query_param(url, "limit"))).await?,
            200,
        ),
        (worker::Method::Get, _) if mastodon_account_following_path(path) => api_json(
            &mastodon_following(&env, clamp_limit(query_param(url, "limit"))).await?,
            200,
        ),
        (worker::Method::Get, _) if mastodon_account_path(path) => {
            api_json(&mastodon_account(&env).await?, 200)
        }
        (worker::Method::Post, _) if mastodon_account_action_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let (id, action) = mastodon_account_action_path(path).unwrap_or_default();
            mastodon_account_action(&env, &decode_component(&id), &action).await
        }
        (worker::Method::Get, _) if mastodon_status_context_path(path).is_some() => {
            let id = mastodon_status_context_path(path).unwrap_or_default();
            match mastodon_status_context(&env, &decode_component(&id)).await? {
                Some(value) => api_json(&value, 200),
                None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
            }
        }
        (worker::Method::Get, _) if mastodon_status_source_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_status_source_path(path).unwrap_or_default();
            match mastodon_status_row(&env, &decode_component(&id)).await? {
                Some(row) => api_json(&mastodon_status_source_json(&row), 200),
                None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
            }
        }
        (worker::Method::Post, _) if mastodon_status_action_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let (id, action) = mastodon_status_action_path(path).unwrap_or_default();
            mastodon_status_action(&env, &decode_component(&id), &action).await
        }
        (worker::Method::Post, "/api/v1/media") | (worker::Method::Post, "/api/v2/media") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            if request_content_type(&req).contains("multipart/form-data") {
                return mastodon_upload_media_multipart(&env, &mut req).await;
            }
            let body = read_mastodon_body(&mut req).await;
            mastodon_upload_media(&env, &body).await
        }
        (worker::Method::Get, _) if mastodon_media_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_media_path(path).unwrap_or_default();
            match mastodon_media_attachment_for_id(&env, &decode_component(&id)).await? {
                Some(attachment) => api_json(&attachment, 200),
                None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
            }
        }
        (worker::Method::Put, _) | (worker::Method::Patch, _)
            if mastodon_media_path(path).is_some() =>
        {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_media_path(path).unwrap_or_default();
            let body = read_mastodon_body(&mut req).await;
            let description = body.get("description").and_then(optional_body_string);
            match mastodon_update_media_attachment(&env, &decode_component(&id), description)
                .await?
            {
                Some(attachment) => api_json(&attachment, 200),
                None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
            }
        }
        (worker::Method::Get, _) if mastodon_status_path(path).is_some() => {
            let id = mastodon_status_path(path).unwrap_or_default();
            match mastodon_status(&env, &decode_component(&id)).await? {
                Some(value) => api_json(&value, 200),
                None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
            }
        }
        (worker::Method::Get, _) if path.starts_with("/api/v1/streaming") => {
            mastodon_streaming_response()
        }
        (worker::Method::Post, "/api/v1/reports") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let body = read_mastodon_body(&mut req).await;
            api_json(&mastodon_report(&body), 201)
        }
        (worker::Method::Post, "/api/v1/statuses") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let body = read_mastodon_body(&mut req).await;
            mastodon_create_status(&env, &body).await
        }
        (worker::Method::Post, "/api/v1/notifications/clear") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            mastodon_clear_notifications(&env).await
        }
        (worker::Method::Post, _) if mastodon_notification_dismiss_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_notification_dismiss_path(path).unwrap_or_default();
            mastodon_dismiss_notification(&env, &decode_component(&id)).await
        }
        (worker::Method::Put, _) | (worker::Method::Patch, _)
            if mastodon_status_path(path).is_some() =>
        {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_status_path(path).unwrap_or_default();
            let body = read_mastodon_body(&mut req).await;
            mastodon_update_status(&env, &decode_component(&id), &body).await
        }
        (worker::Method::Delete, _) if mastodon_status_path(path).is_some() => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            let id = mastodon_status_path(path).unwrap_or_default();
            mastodon_delete_status(&env, &decode_component(&id)).await
        }
        (worker::Method::Delete, _) if mastodon_suggestion_dismiss(path) => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(&serde_json::json!({}), 200)
        }
        _ => api_json(
            &serde_json::json!({ "error": "Not implemented in dais Mastodon compatibility API" }),
            404,
        ),
    }
}

fn oauth_authorization_server_metadata(url: &worker::Url) -> Result<Response> {
    let origin = origin(url);
    api_json(
        &serde_json::json!({
            "issuer": origin,
            "authorization_endpoint": format!("{origin}/oauth/authorize"),
            "token_endpoint": format!("{origin}/oauth/token"),
            "revocation_endpoint": format!("{origin}/oauth/revoke"),
            "scopes_supported": ["read", "write", "follow", "push"],
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code"],
            "token_endpoint_auth_methods_supported": ["client_secret_post", "client_secret_basic", "none"],
            "code_challenge_methods_supported": ["S256", "plain"],
            "service_documentation": "https://github.com/marctjones/dais",
        }),
        200,
    )
}

fn nodeinfo_discovery(url: &worker::Url) -> Result<Response> {
    let origin = origin(url);
    api_json(
        &serde_json::json!({
            "links": [
                {
                    "rel": "http://nodeinfo.diaspora.software/ns/schema/2.0",
                    "href": format!("{origin}/nodeinfo/2.0"),
                }
            ]
        }),
        200,
    )
}

async fn nodeinfo_document(env: &Env) -> Result<Value> {
    Ok(serde_json::json!({
        "version": "2.0",
        "software": {
            "name": "dais",
            "version": "1.28",
            "repository": "https://github.com/marctjones/dais",
        },
        "protocols": ["activitypub"],
        "services": {
            "inbound": [],
            "outbound": [],
        },
        "openRegistrations": false,
        "usage": {
            "users": {
                "total": 1,
                "activeMonth": 1,
                "activeHalfyear": 1,
            },
            "localPosts": public_status_count(env).await?,
        },
        "metadata": {
            "nodeName": "dais",
            "privateByDefault": true,
        },
    }))
}

fn activitypub_public_path(env: &Env, path: &str) -> bool {
    let prefix = activitypub_user_prefix(env);
    path == prefix
        || path == format!("{prefix}/outbox")
        || path == format!("{prefix}/followers")
        || path == format!("{prefix}/following")
        || path == format!("{prefix}/followers_synchronization")
        || path == format!("{prefix}/inbox")
        || path.starts_with(&format!("{prefix}/posts/"))
}

async fn handle_activitypub_public(
    mut req: Request,
    env: Env,
    url: &worker::Url,
) -> Result<Response> {
    let path = url.path();
    let prefix = activitypub_user_prefix(&env);
    let outbox_path = format!("{prefix}/outbox");
    let followers_path = format!("{prefix}/followers");
    let following_path = format!("{prefix}/following");
    let followers_sync_path = format!("{prefix}/followers_synchronization");
    let inbox_path = format!("{prefix}/inbox");
    let posts_prefix = format!("{prefix}/posts/");

    match req.method() {
        worker::Method::Get if path == prefix => {
            if query_param(url, "format").as_deref() == Some("json") || accepts_activity_json(&req)
            {
                activitypub_actor(&env, url).await
            } else {
                activitypub_actor_html(&env, url).await
            }
        }
        worker::Method::Get if path == outbox_path => activitypub_outbox(&env, url).await,
        worker::Method::Get if path == followers_path || path == following_path => {
            activitypub_graph_collection(&env, url, path.trim_start_matches(&format!("{prefix}/")))
                .await
        }
        worker::Method::Get if path == followers_sync_path => {
            activitypub_followers_synchronization(&env, url, &req).await
        }
        worker::Method::Options if path == inbox_path => activitypub_inbox_options(),
        worker::Method::Post if path == inbox_path => activitypub_inbox_post(&env, &mut req).await,
        worker::Method::Get if path.starts_with(&posts_prefix) => {
            activitypub_post(&env, url, &req).await
        }
        _ => Response::error("Not found", 404),
    }
}

fn activitypub_webfinger(env: &Env, url: &worker::Url) -> Result<Response> {
    let resource = query_param(url, "resource").unwrap_or_default();
    let normalized = resource.trim().to_ascii_lowercase();
    let username = local_username(env);
    let accepted_apex = format!("acct:{}@{}", username, handle_domain(env)).to_ascii_lowercase();
    let accepted_activitypub =
        format!("acct:{}@{}", username, activitypub_domain(env)).to_ascii_lowercase();
    if normalized != accepted_apex
        && normalized != accepted_activitypub
        && !normalized.starts_with(&format!("acct:{username}@router-rust-candidate."))
    {
        return Response::error("Resource not found", 404);
    }
    let actor_url = local_actor_url_for_request(env, url);
    jrd_json(
        &serde_json::json!({
            "subject": resource,
            "aliases": [actor_url],
            "links": [
                {
                    "rel": "self",
                    "type": "application/activity+json",
                    "href": actor_url,
                }
            ],
        }),
        200,
    )
}

async fn activitypub_actor(env: &Env, url: &worker::Url) -> Result<Response> {
    let origin = origin(url);
    let row = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type, display_name,
                   summary, avatar_url, header_url, icon, image, public_key
            FROM actors
            WHERE username = 'social'
            LIMIT 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let username = string_field(row.as_ref(), "username").unwrap_or_else(|| "social".to_string());
    let actor_url = format!("{origin}/users/{username}");
    let display_name =
        string_field(row.as_ref(), "display_name").unwrap_or_else(|| username.clone());
    let summary = string_field(row.as_ref(), "summary").unwrap_or_default();
    let public_key = string_field(row.as_ref(), "public_key").unwrap_or_default();
    let icon =
        string_field(row.as_ref(), "icon").or_else(|| string_field(row.as_ref(), "avatar_url"));
    let image =
        string_field(row.as_ref(), "image").or_else(|| string_field(row.as_ref(), "header_url"));

    let mut actor = serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://w3id.org/security/v1"
        ],
        "id": actor_url,
        "type": string_field(row.as_ref(), "actor_type").unwrap_or_else(|| "Person".to_string()),
        "preferredUsername": username,
        "name": display_name,
        "summary": summary,
        "url": format!("{origin}/@{username}"),
        "inbox": format!("{actor_url}/inbox"),
        "outbox": format!("{actor_url}/outbox"),
        "followers": format!("{actor_url}/followers"),
        "following": format!("{actor_url}/following"),
        "manuallyApprovesFollowers": true,
        "discoverable": false,
        "publicKey": {
            "id": format!("{actor_url}#main-key"),
            "owner": actor_url,
            "publicKeyPem": public_key,
        },
        "endpoints": {
            "sharedInbox": format!("{actor_url}/inbox"),
        },
    });
    if let Value::Object(ref mut object) = actor {
        if let Some(icon) = icon {
            object.insert(
                "icon".to_string(),
                serde_json::json!({ "type": "Image", "mediaType": media_type_for_filename(&icon), "url": icon }),
            );
        }
        if let Some(image) = image {
            object.insert(
                "image".to_string(),
                serde_json::json!({ "type": "Image", "mediaType": media_type_for_filename(&image), "url": image }),
            );
        }
        match public_e2ee_devices(env, &actor_url).await {
            Ok(devices) if !devices.is_empty() => {
                object.insert(
                    "daisE2ee".to_string(),
                    serde_json::json!({
                        "v": 1,
                        "protocol": "dais-mls-v1",
                        "devices": devices,
                    }),
                );
            }
            Ok(_) => {}
            Err(error) => {
                worker::console_log!("Skipping public E2EE devices on actor document: {}", error);
            }
        }
    }
    activity_json(&actor)
}

async fn activitypub_actor_html(env: &Env, url: &worker::Url) -> Result<Response> {
    let profile = owner_profile(env).await?;
    let posts = mastodon_status_rows(env, "posts", 20, url).await?;
    text_response(
        &activitypub_actor_profile_html(&profile, &posts),
        "text/html; charset=utf-8",
    )
}

fn activitypub_actor_profile_html(profile: &OwnerProfile, posts: &[Map<String, Value>]) -> String {
    let display_name = profile
        .display_name
        .clone()
        .unwrap_or_else(|| profile.username.clone());
    let summary = profile.summary.clone().unwrap_or_default();
    let posts_html = if posts.is_empty() {
        "<p>No public posts yet.</p>".to_string()
    } else {
        posts
            .iter()
            .map(activitypub_actor_post_html)
            .collect::<Vec<_>>()
            .join("")
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;max-width:760px;margin:40px auto;padding:0 20px;line-height:1.5;color:#111827}}a{{color:#0f766e}}article{{border-top:1px solid #d1d5db;padding:18px 0}}time{{color:#6b7280;font-size:.9rem}}.summary{{color:#374151}}</style></head><body><header><h1>{}</h1><p class=\"summary\">{}</p><p><a rel=\"alternate\" type=\"application/activity+json\" href=\"/users/{}/outbox\">ActivityPub outbox</a></p></header><main><h2>Public posts</h2>{}</main></body></html>",
        escape_html(&display_name),
        escape_html(&display_name),
        escape_html(&summary),
        escape_html(&profile.username),
        posts_html,
    )
}

fn activitypub_actor_post_html(row: &Map<String, Value>) -> String {
    let id = string_field(Some(row), "id").unwrap_or_default();
    let published = string_field(Some(row), "published_at").unwrap_or_default();
    let permalink = if id.starts_with("http://") || id.starts_with("https://") {
        id
    } else {
        format!("/users/social/posts/{}", escape_html(&id))
    };
    format!(
        "<article><time>{}</time><div>{}</div><p><a href=\"{}\">Permalink</a></p></article>",
        escape_html(&published),
        mastodon_status_content(row),
        escape_html(&permalink),
    )
}

async fn activitypub_outbox(env: &Env, url: &worker::Url) -> Result<Response> {
    let origin = origin(url);
    let actor_url = local_actor_url_for_request(env, url);
    let rows = mastodon_status_rows(env, "posts", 20, url).await?;
    let total = public_status_count(env).await?;
    let ordered_items = rows
        .iter()
        .map(|row| {
            let object = activitypub_note_object(row, &origin);
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(&actor_url)
                .to_string();
            serde_json::json!({
                "id": format!("{id}#create"),
                "type": "Create",
                "actor": actor_url,
                "published": row_value_or_null(row, "published_at"),
                "to": [PUBLIC_COLLECTION],
                "cc": [format!("{actor_url}/followers")],
                "object": object,
            })
        })
        .collect::<Vec<_>>();
    activity_json(&serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{actor_url}/outbox"),
        "type": "OrderedCollection",
        "totalItems": total,
        "orderedItems": ordered_items,
    }))
}

async fn activitypub_post(env: &Env, url: &worker::Url, req: &Request) -> Result<Response> {
    let origin = origin(url);
    let id = format!("{origin}{}", url.path());
    let row = match mastodon_status_row(env, &id).await? {
        Some(row) => row,
        None => {
            let Some(row) = activitypub_any_post_row(env, &id).await? else {
                return Response::error("Not found", 404);
            };
            if !signed_approved_follower(env, req).await? {
                return Response::error("Not found", 404);
            }
            row
        }
    };
    if !accepts_activity_json(req) && query_param(url, "format").as_deref() != Some("json") {
        let content = mastodon_status_content(&row);
        return text_response(
            &format!(
                "<!doctype html><html><head><meta charset=\"utf-8\"><title>dais post</title></head><body>{}</body></html>",
                content,
            ),
            "text/html; charset=utf-8",
        );
    }
    activity_json(&activitypub_note_object(&row, &origin))
}

async fn activitypub_any_post_row(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let canonical_id = canonical_mastodon_status_id(id);
    let id_arg = D1Type::Text(&canonical_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
                   (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
            FROM posts
            WHERE id = ?1
              AND encrypted_message IS NULL
            LIMIT 1
            "#,
        )
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await
}

async fn signed_approved_follower(env: &Env, req: &Request) -> Result<bool> {
    let Some(actor_id) = signature_actor_id(req)? else {
        return Ok(false);
    };
    let actor_arg = D1Type::Text(&actor_id);
    Ok(env
        .d1("DB")?
        .prepare(
            r#"
            SELECT 1 AS allowed
            FROM followers
            WHERE follower_actor_id = ?1 AND status = 'approved'
            LIMIT 1
            "#,
        )
        .bind_refs(&actor_arg)?
        .first::<Map<String, Value>>(None)
        .await?
        .is_some())
}

async fn activitypub_graph_collection(
    env: &Env,
    url: &worker::Url,
    name: &str,
) -> Result<Response> {
    let count_query = if name == "following" {
        "SELECT COUNT(*) AS count FROM following WHERE status = 'accepted'"
    } else {
        "SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'"
    };
    let row = env
        .d1("DB")?
        .prepare(count_query)
        .first::<Map<String, Value>>(None)
        .await?;
    let origin = origin(url);
    let collection_url = format!("{origin}/users/social/{name}");
    let first = format!("{collection_url}?page=1");
    let mut body = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": collection_url,
        "type": "OrderedCollection",
        "totalItems": integer_field(row.as_ref(), "count"),
        "first": first,
    });
    if query_param(url, "page").is_some() {
        body = serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": first,
            "partOf": collection_url,
            "type": "OrderedCollectionPage",
            "totalItems": integer_field(row.as_ref(), "count"),
            "orderedItems": [],
        });
    }
    activity_json(&body)
}

async fn activitypub_followers_synchronization(
    env: &Env,
    url: &worker::Url,
    req: &Request,
) -> Result<Response> {
    let Some(actor_id) = signature_actor_id(req)? else {
        return activitypub_error("HTTP Signature required", 401);
    };
    let domain = query_param(url, "domain")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if domain.is_empty() || actor_domain(&actor_id) != domain {
        return activitypub_error("Signature actor must be on requested domain", 403);
    }
    let local_actor = owner_local_actor(env).await?;
    let actor_arg = D1Type::Text(&local_actor.id);
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT follower_actor_id
            FROM followers
            WHERE actor_id = ?1 AND status = 'approved'
            ORDER BY follower_actor_id ASC
            "#,
        )
        .bind_refs(&actor_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let mut ordered_items = Vec::new();
    for row in rows {
        let Some(follower) = string_field(Some(&row), "follower_actor_id") else {
            continue;
        };
        if actor_domain(&follower) == domain && !ordered_items.contains(&follower) {
            ordered_items.push(follower);
        }
    }
    activity_json(&serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!(
            "{}/users/social/followers_synchronization?domain={}",
            origin(url),
            urlencoding::encode(&domain)
        ),
        "type": "OrderedCollection",
        "totalItems": ordered_items.len(),
        "orderedItems": ordered_items,
    }))
}

fn activitypub_note_object(row: &Map<String, Value>, origin: &str) -> Value {
    let id = display_local_url(origin, &string_field(Some(row), "id").unwrap_or_default());
    let actor = format!("{origin}/users/social");
    let content = mastodon_status_content(row);
    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": string_field(Some(row), "object_type").unwrap_or_else(|| "Note".to_string()),
        "url": id,
        "attributedTo": actor,
        "content": content,
        "contentMap": { "en": content },
        "published": row_value_or_null(row, "published_at"),
        "to": [PUBLIC_COLLECTION],
        "cc": [format!("{actor}/followers")],
        "replies": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "reply_count"),
        },
        "likes": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "like_count"),
        },
        "shares": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "boost_count"),
        },
    });
    if let Value::Object(ref mut object) = note {
        insert_optional_activity_string(object, "name", string_field(Some(row), "name"));
        insert_optional_activity_string(object, "summary", string_field(Some(row), "summary"));
        if let Some(reply) = string_field(Some(row), "in_reply_to") {
            object.insert(
                "inReplyTo".to_string(),
                Value::String(display_local_url(origin, &reply)),
            );
        }
        let attachments = activitypub_attachments(row);
        if !attachments.is_empty() {
            object.insert("attachment".to_string(), Value::Array(attachments));
        }
        let tags = activitypub_tags(row);
        if !tags.is_empty() {
            object.insert("tag".to_string(), Value::Array(tags));
        }
        if let Some(poll) = activitypub_poll(row) {
            let multiple = poll
                .get("multiple")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let key = if multiple { "anyOf" } else { "oneOf" };
            let options = poll
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .map(|name| {
                    serde_json::json!({
                        "type": "Note",
                        "name": name,
                        "replies": { "type": "Collection", "totalItems": 0 },
                    })
                })
                .collect::<Vec<_>>();
            object.insert(key.to_string(), Value::Array(options));
            object.insert("votersCount".to_string(), Value::from(0));
        }
    }
    note
}

fn insert_optional_activity_string(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn activitypub_attachments(row: &Map<String, Value>) -> Vec<Value> {
    parse_attachment_array(row.get("media_attachments"))
        .into_iter()
        .filter_map(|attachment| {
            let object = attachment.as_object()?;
            let url = string_field(Some(object), "url")?;
            let media_type = string_field(Some(object), "mediaType")
                .unwrap_or_else(|| media_type_for_filename(&url));
            let attachment_type = if media_type.starts_with("image/") {
                "Image"
            } else {
                "Document"
            };
            let mut item = Map::new();
            item.insert(
                "type".to_string(),
                Value::String(attachment_type.to_string()),
            );
            item.insert("url".to_string(), Value::String(url));
            item.insert("mediaType".to_string(), Value::String(media_type));
            insert_optional_activity_string(&mut item, "name", string_field(Some(object), "name"));
            Some(Value::Object(item))
        })
        .collect()
}

fn activitypub_poll(row: &Map<String, Value>) -> Option<Value> {
    if string_field(Some(row), "object_type").as_deref() != Some("Question") {
        return None;
    }
    match row.get("poll_options")? {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        value => Some(value.clone()),
    }
}

fn activitypub_tags(row: &Map<String, Value>) -> Vec<Value> {
    let mut tags = Vec::new();
    if let Value::Array(mentions) = mastodon_mentions(row) {
        for mention in mentions {
            let Some(mention) = mention.as_object() else {
                continue;
            };
            let Some(acct) = string_field(Some(mention), "acct") else {
                continue;
            };
            tags.push(serde_json::json!({
                "type": "Mention",
                "name": format!("@{acct}"),
                "href": string_field(Some(mention), "url").unwrap_or_default(),
            }));
        }
    }
    if let Value::Array(hashtags) = mastodon_tags(row) {
        for hashtag in hashtags {
            let Some(hashtag) = hashtag.as_object() else {
                continue;
            };
            let Some(name) = string_field(Some(hashtag), "name") else {
                continue;
            };
            tags.push(serde_json::json!({
                "type": "Hashtag",
                "name": format!("#{name}"),
                "href": string_field(Some(hashtag), "url").unwrap_or_default(),
            }));
        }
    }
    tags
}

fn display_local_url(origin: &str, value: &str) -> String {
    let origin_host = worker::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned));
    worker::Url::parse(value)
        .ok()
        .and_then(|url| {
            let path = url.path();
            (url.host_str() == origin_host.as_deref() && path.starts_with("/users/social/"))
                .then(|| format!("{origin}{path}"))
        })
        .unwrap_or_else(|| value.to_string())
}

fn accepts_activity_json(req: &Request) -> bool {
    req.headers()
        .get("Accept")
        .ok()
        .flatten()
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("activity+json")
                || value.contains("application/ld+json")
                || value.contains("application/json")
        })
        .unwrap_or(false)
}

fn signature_actor_id(req: &Request) -> Result<Option<String>> {
    let Some(header) = req.headers().get("Signature")? else {
        return Ok(None);
    };
    let Some(key_id) = signature_header_value(&header, "keyId") else {
        return Ok(None);
    };
    let actor_id = key_id
        .split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok((!actor_id.is_empty()).then_some(actor_id))
}

fn signature_header_value(header: &str, key: &str) -> Option<String> {
    for part in header.split(',') {
        let mut pieces = part.splitn(2, '=');
        let name = pieces.next()?.trim();
        let value = pieces.next()?.trim().trim_matches('"');
        if name.eq_ignore_ascii_case(key) && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn actor_domain(actor_id: &str) -> String {
    worker::Url::parse(actor_id)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn activitypub_inbox_options() -> Result<Response> {
    let headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set(
        "Access-Control-Allow-Headers",
        "Authorization, Content-Type, Date, Digest, Signature",
    )?;
    headers.set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")?;
    Ok(Response::empty()?.with_status(200).with_headers(headers))
}

async fn activitypub_inbox_post(env: &Env, req: &mut Request) -> Result<Response> {
    if req.headers().get("Signature")?.is_none() {
        return activitypub_error("HTTP Signature required", 401);
    }
    let body = read_json(req).await;
    let activity_type = body.get("type").and_then(Value::as_str).unwrap_or_default();
    match activity_type {
        "Follow" => {
            if let Err(message) = activitypub_store_follow(env, &body).await {
                return activitypub_error(&message, 400);
            }
        }
        "Accept" => {
            if let Err(message) = activitypub_store_accept(env, &body).await {
                return activitypub_error(&message, 400);
            }
        }
        "Create" => {
            if let Err(message) = activitypub_store_create(env, &body).await {
                return activitypub_error(&message, 400);
            }
        }
        "Delete" => {
            if let Err(message) = activitypub_store_delete(env, &body).await {
                return activitypub_error(&message, 400);
            }
        }
        "Undo" => {
            if let Err(message) = activitypub_store_undo(env, &body).await {
                return activitypub_error(&message, 400);
            }
        }
        _ => {}
    }
    api_json(&serde_json::json!({ "accepted": true }), 202)
}

async fn activitypub_store_follow(env: &Env, body: &Value) -> std::result::Result<(), String> {
    let actor = body
        .get("actor")
        .and_then(optional_body_string)
        .ok_or_else(|| "Follow actor is required".to_string())?;
    let follow_id = body
        .get("id")
        .and_then(optional_body_string)
        .unwrap_or_else(|| format!("{actor}#follow-{}", stable_id(&actor)));
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let remote = resolve_activitypub_actor_for_local(&actor, &local_actor).await?;
    let inbox = if remote.inbox.is_empty() {
        format!("{actor}/inbox")
    } else {
        remote.inbox
    };
    let shared_inbox = remote.shared_inbox.unwrap_or_else(|| inbox.clone());
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&follow_id);
    let local_arg = D1Type::Text(&local_actor.id);
    let actor_arg = D1Type::Text(&actor);
    let inbox_arg = D1Type::Text(&inbox);
    let shared_arg = D1Type::Text(&shared_inbox);
    db.prepare(
        r#"
        INSERT INTO followers (
            id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending')
        ON CONFLICT(actor_id, follower_actor_id) DO UPDATE SET
            id = excluded.id,
            follower_inbox = excluded.follower_inbox,
            follower_shared_inbox = excluded.follower_shared_inbox,
            status = CASE WHEN followers.status = 'approved' THEN 'approved' ELSE 'pending' END,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([&id_arg, &local_arg, &actor_arg, &inbox_arg, &shared_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn activitypub_store_undo(env: &Env, body: &Value) -> std::result::Result<(), String> {
    let actor = body
        .get("actor")
        .and_then(optional_body_string)
        .ok_or_else(|| "Undo actor is required".to_string())?;
    let object_type = body
        .get("object")
        .and_then(Value::as_object)
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if object_type != "Follow" {
        return Ok(());
    }
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let local_arg = D1Type::Text(&local_actor.id);
    let actor_arg = D1Type::Text(&actor);
    db.prepare("DELETE FROM followers WHERE actor_id = ?1 AND follower_actor_id = ?2")
        .bind_refs([&local_arg, &actor_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn activitypub_store_accept(env: &Env, body: &Value) -> std::result::Result<(), String> {
    let actor = body
        .get("actor")
        .and_then(optional_body_string)
        .ok_or_else(|| "Accept actor is required".to_string())?;
    let object_type = body
        .get("object")
        .and_then(Value::as_object)
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if object_type != "Follow" {
        return Ok(());
    }
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let actor_arg = D1Type::Text(&actor);
    db.prepare(
        "UPDATE following SET status = 'accepted', accepted_at = CURRENT_TIMESTAMP WHERE target_actor_id = ?1 AND status = 'pending'",
    )
    .bind_refs(&actor_arg)
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn activitypub_store_create(env: &Env, body: &Value) -> std::result::Result<(), String> {
    let actor = body
        .get("actor")
        .and_then(optional_body_string)
        .ok_or_else(|| "Create actor is required".to_string())?;
    let object = body
        .get("object")
        .and_then(Value::as_object)
        .ok_or_else(|| "Create object is required".to_string())?;
    let object_id = object
        .get("id")
        .and_then(optional_body_string)
        .ok_or_else(|| "Create object id is required".to_string())?;
    let object_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !supported_timeline_object_type(object_type) {
        return Ok(());
    }
    let remote = resolve_activitypub_actor(&actor).await.ok();
    let username = remote
        .as_ref()
        .and_then(|actor| actor.preferred_username.clone())
        .unwrap_or_else(|| parse_actor_acct(&actor).0);
    let display_name = remote
        .as_ref()
        .and_then(|actor| actor.name.clone())
        .unwrap_or_else(|| username.clone());
    let avatar = remote.and_then(|actor| actor.icon_url).unwrap_or_default();
    let content_html = activitypub_object_content_html(object);
    let content = strip_html(&content_html);
    let visibility = if activitypub_public_recipients(body, &Value::Object(object.clone())) {
        "public"
    } else {
        "followers"
    };
    let published = object
        .get("published")
        .and_then(optional_body_string)
        .or_else(|| body.get("published").and_then(optional_body_string))
        .unwrap_or_else(|| {
            js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default()
        });
    let in_reply_to = object
        .get("inReplyTo")
        .and_then(|value| value_string(Some(value)))
        .unwrap_or_default();
    let raw_object = Value::Object(object.clone()).to_string();
    let raw_activity = body.to_string();
    let id = format!("timeline-{}", stable_id(&object_id));
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&id);
    let object_arg = D1Type::Text(&object_id);
    let actor_arg = D1Type::Text(&actor);
    let username_arg = D1Type::Text(&username);
    let display_arg = D1Type::Text(&display_name);
    let avatar_arg = D1Type::Text(&avatar);
    let content_arg = D1Type::Text(&content);
    let html_arg = D1Type::Text(&content_html);
    let visibility_arg = D1Type::Text(visibility);
    let reply_arg = if in_reply_to.is_empty() {
        D1Type::Null
    } else {
        D1Type::Text(&in_reply_to)
    };
    let published_arg = D1Type::Text(&published);
    let object_json_arg = D1Type::Text(&raw_object);
    let activity_json_arg = D1Type::Text(&raw_activity);
    db.prepare(
        r#"
        INSERT INTO timeline_posts (
            id, object_id, actor_id, actor_username, actor_display_name,
            actor_avatar_url, content, content_html, visibility, in_reply_to,
            published_at, raw_object, raw_activity, protocol
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'activitypub')
        ON CONFLICT(object_id) DO UPDATE SET
            actor_id = excluded.actor_id,
            actor_username = excluded.actor_username,
            actor_display_name = excluded.actor_display_name,
            actor_avatar_url = excluded.actor_avatar_url,
            content = excluded.content,
            content_html = excluded.content_html,
            visibility = excluded.visibility,
            in_reply_to = excluded.in_reply_to,
            published_at = excluded.published_at,
            raw_object = excluded.raw_object,
            raw_activity = excluded.raw_activity,
            deleted_at = NULL
        "#,
    )
    .bind_refs([
        &id_arg,
        &object_arg,
        &actor_arg,
        &username_arg,
        &display_arg,
        &avatar_arg,
        &content_arg,
        &html_arg,
        &visibility_arg,
        &reply_arg,
        &published_arg,
        &object_json_arg,
        &activity_json_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    if !in_reply_to.is_empty() && is_local_object_url(&in_reply_to, &activitypub_domain(env)) {
        let activity_id = body
            .get("id")
            .and_then(optional_body_string)
            .unwrap_or_else(|| object_id.clone());
        activitypub_store_local_reply(
            env,
            &object_id,
            &in_reply_to,
            &actor,
            &username,
            &display_name,
            &avatar,
            &content,
            &content_html,
            visibility,
            &published,
            &activity_id,
        )
        .await?;
    }

    if activitypub_direct_to_actor(&Value::Object(object.clone()), &local_actor_url(env)) {
        if let Some(encrypted_message) = object.get("daisEncryptedMessage") {
            validate_dais_encrypted_message_v2(encrypted_message)?;
            activitypub_store_e2ee_direct_message(
                env,
                &actor,
                &object_id,
                &Value::Object(object.clone()),
                &published,
                &content,
                encrypted_message,
                "daisEncryptedMessage",
                "mls-rfc9420",
            )
            .await?;
        } else if let Some(encrypted_message) = object.get("encryptedMessage") {
            activitypub_store_e2ee_direct_message(
                env,
                &actor,
                &object_id,
                &Value::Object(object.clone()),
                &published,
                &content,
                encrypted_message,
                "encryptedMessage",
                "dais-mls-v1",
            )
            .await?;
        }
    }
    Ok(())
}

async fn activitypub_store_local_reply(
    env: &Env,
    reply_id: &str,
    post_id: &str,
    actor: &str,
    username: &str,
    display_name: &str,
    avatar: &str,
    content: &str,
    content_html: &str,
    visibility: &str,
    published: &str,
    activity_id: &str,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let post_arg = D1Type::Text(post_id);
    let existing = db
        .prepare("SELECT id FROM posts WHERE id = ?1 LIMIT 1")
        .bind_refs(&post_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Ok(());
    }

    let reply_arg = D1Type::Text(reply_id);
    let actor_arg = D1Type::Text(actor);
    let username_arg = D1Type::Text(username);
    let display_arg = D1Type::Text(display_name);
    let avatar_arg = D1Type::Text(avatar);
    let html_arg = D1Type::Text(content_html);
    let visibility_arg = D1Type::Text(visibility);
    let published_arg = D1Type::Text(published);
    db.prepare(
        r#"
        INSERT INTO replies (
            id, post_id, actor_id, actor_username, actor_display_name,
            actor_avatar_url, content, published_at, visibility
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            post_id = excluded.post_id,
            actor_id = excluded.actor_id,
            actor_username = excluded.actor_username,
            actor_display_name = excluded.actor_display_name,
            actor_avatar_url = excluded.actor_avatar_url,
            content = excluded.content,
            published_at = excluded.published_at,
            visibility = excluded.visibility
        "#,
    )
    .bind_refs([
        &reply_arg,
        &post_arg,
        &actor_arg,
        &username_arg,
        &display_arg,
        &avatar_arg,
        &html_arg,
        &published_arg,
        &visibility_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let notification_id = format!("notification-reply-{}", stable_id(reply_id));
    let notification_arg = D1Type::Text(&notification_id);
    let activity_arg = D1Type::Text(activity_id);
    let content_arg = D1Type::Text(content);
    db.prepare(
        r#"
        INSERT INTO notifications (
            id, type, actor_id, actor_username, actor_display_name,
            actor_avatar_url, post_id, activity_id, content, read, created_at
        ) VALUES (?1, 'reply', ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
        ON CONFLICT(id) DO UPDATE SET
            actor_id = excluded.actor_id,
            actor_username = excluded.actor_username,
            actor_display_name = excluded.actor_display_name,
            actor_avatar_url = excluded.actor_avatar_url,
            post_id = excluded.post_id,
            activity_id = excluded.activity_id,
            content = excluded.content,
            created_at = excluded.created_at
        "#,
    )
    .bind_refs([
        &notification_arg,
        &actor_arg,
        &username_arg,
        &display_arg,
        &avatar_arg,
        &post_arg,
        &activity_arg,
        &content_arg,
        &published_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn activitypub_store_e2ee_direct_message(
    env: &Env,
    actor: &str,
    object_id: &str,
    object: &Value,
    published: &str,
    fallback_content: &str,
    encrypted_message: &Value,
    envelope_field: &str,
    protocol: &str,
) -> std::result::Result<(), String> {
    let local_actor = local_actor_url(env);
    let mut participants = vec![actor.to_string(), local_actor.clone()];
    participants.sort();
    let participants_json =
        serde_json::to_string(&participants).map_err(|error| error.to_string())?;
    let conversation_id = format!("e2ee-conversation-{}", stable_id(&participants.join("\n")));
    let sender_device_id = object
        .get("daisE2ee")
        .and_then(|value| value.get("senderDeviceId"))
        .and_then(Value::as_str)
        .or_else(|| object.get("senderDeviceId").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let encrypted_json =
        serde_json::to_string(encrypted_message).map_err(|error| error.to_string())?;
    let attachments = encrypted_media_attachments_from_activitypub_object(object)?;
    let aad_json = serde_json::to_string(&serde_json::json!({
        "recipientActorId": local_actor,
        "fallbackContent": fallback_content,
        "e2eeProtocol": protocol,
        "e2eeField": envelope_field,
        "attachments": attachments,
    }))
    .map_err(|error| error.to_string())?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let conversation_arg = D1Type::Text(&conversation_id);
    let participants_arg = D1Type::Text(&participants_json);
    let published_arg = D1Type::Text(published);
    db.prepare(
        r#"
        INSERT INTO e2ee_conversations (id, protocol, participants, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(id) DO UPDATE SET protocol = excluded.protocol, updated_at = ?4
        "#,
    )
    .bind_refs(&[
        conversation_arg,
        D1Type::Text(protocol),
        participants_arg,
        published_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let id_arg = D1Type::Text(object_id);
    let conversation_arg = D1Type::Text(&conversation_id);
    let actor_arg = D1Type::Text(actor);
    let sender_device_arg = D1Type::Text(sender_device_id);
    let encrypted_arg = D1Type::Text(&encrypted_json);
    let aad_arg = D1Type::Text(&aad_json);
    let published_arg = D1Type::Text(published);
    db.prepare(
        r#"
        INSERT OR IGNORE INTO e2ee_messages (
            id, conversation_id, sender_actor_id, sender_device_id, ciphertext, aad, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind_refs(&[
        id_arg,
        conversation_arg,
        actor_arg,
        sender_device_arg,
        encrypted_arg,
        aad_arg,
        published_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    if protocol == "mls-rfc9420" {
        persist_mls_message_metadata(
            env,
            object_id,
            &conversation_id,
            encrypted_message,
            actor,
            sender_device_id,
            published,
        )
        .await?;
    }
    Ok(())
}

async fn persist_mls_message_metadata(
    env: &Env,
    message_id: &str,
    conversation_id: &str,
    encrypted_message: &Value,
    sender_actor_id: &str,
    sender_device_id: &str,
    received_at: &str,
) -> std::result::Result<(), String> {
    let group_id = encrypted_message
        .get("groupId")
        .and_then(Value::as_str)
        .ok_or("daisEncryptedMessage.groupId is required")?;
    let epoch = encrypted_message
        .get("epoch")
        .and_then(Value::as_u64)
        .ok_or("daisEncryptedMessage.epoch is required")?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    db.prepare(
        r#"
        INSERT OR IGNORE INTO e2ee_mls_message_metadata (
            message_id, conversation_id, group_id, epoch, sender_actor_id,
            sender_device_id, decrypt_status, received_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)
        "#,
    )
    .bind_refs(&[
        D1Type::Text(message_id),
        D1Type::Text(conversation_id),
        D1Type::Text(group_id),
        D1Type::Integer(epoch as i32),
        D1Type::Text(sender_actor_id),
        D1Type::Text(sender_device_id),
        D1Type::Text(received_at),
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn activitypub_store_delete(env: &Env, body: &Value) -> std::result::Result<(), String> {
    let object_id = body
        .get("object")
        .and_then(|value| value_string(Some(value)))
        .or_else(|| {
            body.get("object")
                .and_then(Value::as_object)
                .and_then(|object| object.get("id"))
                .and_then(|value| value_string(Some(value)))
        })
        .ok_or_else(|| "Delete object is required".to_string())?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let object_arg = D1Type::Text(&object_id);
    db.prepare("UPDATE timeline_posts SET deleted_at = CURRENT_TIMESTAMP WHERE object_id = ?1")
        .bind_refs(&object_arg)
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn activitypub_public_recipients(activity: &Value, object: &Value) -> bool {
    let mut recipients = Vec::new();
    collect_recipients(activity.get("to"), &mut recipients);
    collect_recipients(activity.get("cc"), &mut recipients);
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    recipients.iter().any(|value| value == PUBLIC_COLLECTION)
}

fn activitypub_direct_to_actor(object: &Value, actor_url: &str) -> bool {
    let mut recipients = Vec::new();
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    recipients.iter().any(|value| value == actor_url)
        && !recipients.iter().any(|value| value == PUBLIC_COLLECTION)
}

fn supported_timeline_object_type(object_type: &str) -> bool {
    matches!(
        object_type,
        "Note"
            | "Question"
            | "Article"
            | "Page"
            | "Image"
            | "Video"
            | "Audio"
            | "Event"
            | "Document"
            | "Review"
    )
}

fn activitypub_object_content_html(object: &Map<String, Value>) -> String {
    if let Some(content) = object
        .get("content")
        .and_then(|value| value_string(Some(value)))
    {
        return content;
    }
    if let Some(content_map) = object
        .get("contentMap")
        .and_then(Value::as_object)
        .and_then(|map| map.get("en").or_else(|| map.values().next()))
        .and_then(|value| value_string(Some(value)))
    {
        return content_map;
    }

    let object_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("Object");
    let mut parts = Vec::new();
    if let Some(name) = object.get("name").and_then(optional_body_string) {
        parts.push(format!("<p><strong>{}</strong></p>", escape_html(&name)));
    }
    if let Some(summary) = object.get("summary").and_then(optional_body_string) {
        parts.push(format!("<p>{}</p>", escape_html(&summary)));
    }
    if object_type == "Event" {
        if let Some(start) = object.get("startTime").and_then(optional_body_string) {
            parts.push(format!("<p>Starts: {}</p>", escape_html(&start)));
        }
        if let Some(end) = object.get("endTime").and_then(optional_body_string) {
            parts.push(format!("<p>Ends: {}</p>", escape_html(&end)));
        }
        if let Some(location) = object.get("location").and_then(activitypub_location_label) {
            parts.push(format!("<p>Location: {}</p>", escape_html(&location)));
        }
    }
    if parts.is_empty() {
        if let Some(url) = object
            .get("url")
            .or_else(|| object.get("id"))
            .and_then(optional_body_string)
        {
            parts.push(format!(
                "<p>{} from {}</p>",
                escape_html(object_type),
                escape_html(&url)
            ));
        }
    }
    parts.join("")
}

fn activitypub_location_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()).filter(|text| !text.is_empty()),
        Value::Object(object) => object
            .get("name")
            .or_else(|| object.get("address"))
            .and_then(optional_body_string),
        _ => None,
    }
}

fn activitypub_error(message: &str, status: u16) -> Result<Response> {
    api_json(&serde_json::json!({ "error": message }), status)
}

fn jrd_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/jrd+json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    Ok(Response::from_json(value)?
        .with_status(status)
        .with_headers(headers))
}

async fn mastodon_instance(env: &Env, v2: bool) -> Result<Value> {
    let activitypub_domain = activitypub_domain(env);
    let mut instance = serde_json::json!({
        "uri": activitypub_domain.clone(),
        "domain": activitypub_domain.clone(),
        "title": "dais",
        "short_description": "Private-by-default single-user social server",
        "description": "dais speaks ActivityPub and AT Protocol with private-by-default posting.",
        "email": "",
        "version": "4.2.0 (compatible; dais)",
        "registrations": false,
        "approval_required": true,
        "invites_enabled": false,
        "urls": { "streaming_api": format!("wss://{}", activitypub_domain) },
        "stats": {
            "user_count": 1,
            "status_count": public_status_count(env).await?,
            "domain_count": 1,
        },
    });

    if v2 {
        if let Value::Object(ref mut object) = instance {
            object.insert(
                "source_url".to_string(),
                Value::String("https://github.com/marctjones/dais".to_string()),
            );
            object.insert("languages".to_string(), serde_json::json!(["en"]));
            object.insert(
                "configuration".to_string(),
                serde_json::json!({
                    "statuses": {
                        "max_characters": 5000,
                        "max_media_attachments": 4,
                        "characters_reserved_per_url": 23,
                    },
                    "media_attachments": {
                        "supported_mime_types": [
                            "image/jpeg",
                            "image/png",
                            "image/gif",
                            "image/webp",
                            "video/mp4",
                            "video/webm",
                        ],
                    },
                    "polls": {
                        "max_options": 4,
                        "max_characters_per_option": 200,
                        "min_expiration": 300,
                        "max_expiration": 2629746,
                    },
                }),
            );
        }
    }
    Ok(instance)
}

async fn handle_media(req: Request, env: Env, url: &worker::Url) -> Result<Response> {
    let path = url.path();
    let Some(key) = media_r2_key_from_path(path) else {
        return Response::error("Not found", 404);
    };
    if path.starts_with("/media/_private_signed/") {
        if req.headers().get("Signature")?.is_none() {
            return Response::error("HTTP Signature required", 401);
        }
        if !signed_approved_follower(&env, &req).await? {
            return Response::error("Signed media fetch requires an approved follower", 403);
        }
        if !private_media_attached_post(&env, &origin(&url), path).await? {
            return Response::error("Not found", 404);
        }
    } else if path.starts_with("/media/_private/") {
        return Response::error("HTTP Signature required", 401);
    }

    let bucket = env.bucket("MEDIA_BUCKET")?;
    let Some(object) = bucket.get(key.clone()).execute().await? else {
        return Response::error("Not found", 404);
    };
    let custom_metadata = object.custom_metadata()?;
    if media_metadata_is_expired(&custom_metadata, js_sys::Date::now()) {
        bucket.delete(key).await?;
        return Response::error("Not found", 404);
    }
    let bytes = match object.body() {
        Some(body) => body.bytes().await?,
        None => Vec::new(),
    };
    let mut response = Response::from_bytes(bytes)?;
    let headers = Headers::new();
    headers.set(
        "Content-Type",
        &object
            .http_metadata()
            .content_type
            .unwrap_or_else(|| media_type_for_filename(&key)),
    )?;
    headers.set("Cache-Control", "private, max-age=300")?;
    response = response.with_headers(headers);
    Ok(response)
}

fn media_r2_key_from_path(path: &str) -> Option<String> {
    path.strip_prefix("/media/_private_signed/")
        .or_else(|| path.strip_prefix("/media/_private/"))
        .map(|rest| format!("private/{}", decode_component(rest)))
        .or_else(|| {
            path.strip_prefix("/media/uploads/")
                .map(|rest| decode_component(&format!("uploads/{rest}")))
        })
        .filter(|key| !key.trim().is_empty() && !key.contains(".."))
}

async fn private_media_attached_post(
    env: &Env,
    request_origin: &str,
    media_path: &str,
) -> Result<bool> {
    let media_url = format!("{request_origin}{media_path}");
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT media_attachments
            FROM posts
            WHERE visibility IN ('followers', 'direct')
              AND media_attachments IS NOT NULL
              AND media_attachments != ''
            ORDER BY published_at DESC
            LIMIT 250
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    for row in rows {
        for attachment in parse_attachment_array(row.get("media_attachments")) {
            let Some(object) = attachment.as_object() else {
                continue;
            };
            if string_field(Some(object), "url").as_deref() == Some(media_url.as_str()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn mastodon_create_app(body: &Value) -> Value {
    let name = body_string_any(body, &["client_name", "name"])
        .unwrap_or_else(|| "dais client".to_string());
    let redirect_uri = body_string_any(body, &["redirect_uris", "redirect_uri"])
        .unwrap_or_else(|| "urn:ietf:wg:oauth:2.0:oob".to_string());
    serde_json::json!({
        "id": stable_id(&name),
        "name": name,
        "website": body.get("website").and_then(optional_body_string),
        "redirect_uri": redirect_uri,
        "client_id": format!("dais-{}", stable_id(&name)),
        "client_secret": format!("dais-secret-{}", stable_id(&redirect_uri)),
        "vapid_key": "",
    })
}

fn oauth_authorize(url: &worker::Url) -> Result<Response> {
    let redirect_uri = query_param(url, "redirect_uri");
    let state = query_param(url, "state");
    let code = "dais-local-owner";
    if let Some(redirect_uri) = redirect_uri.filter(|value| value != "urn:ietf:wg:oauth:2.0:oob") {
        let mut redirect = worker::Url::parse(&redirect_uri)?;
        redirect.query_pairs_mut().append_pair("code", code);
        if let Some(state) = state {
            redirect.query_pairs_mut().append_pair("state", &state);
        }
        return Response::redirect(redirect);
    }
    text_response(
        &format!("Authorization code: {code}\n"),
        "text/plain; charset=utf-8",
    )
}

fn mastodon_oauth_token(body: &Value) -> Result<Response> {
    let grant_type = body.get("grant_type").and_then(optional_body_string);
    let code = body.get("code").and_then(optional_body_string);
    if grant_type.as_deref() == Some("authorization_code")
        && code
            .as_deref()
            .map(|value| value != "dais-local-owner")
            .unwrap_or(false)
    {
        return api_json(
            &serde_json::json!({
                "error": "invalid_grant",
                "error_description": "authorization code is not valid for this single-user dais server",
            }),
            400,
        );
    }
    let created_at = (js_sys::Date::now() / 1000.0).floor() as i64;
    api_json(
        &serde_json::json!({
            "access_token": "owner-token-required",
            "token_type": "Bearer",
            "scope": "read write follow push",
            "created_at": created_at,
            "dais_owner_token_required": true,
        }),
        200,
    )
}

async fn mastodon_preferences(env: &Env) -> Result<Value> {
    let settings = owner_settings(env).await?;
    let visibility = string_field(Some(&settings), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    Ok(serde_json::json!({
        "posting:default:visibility": mastodon_visibility(&visibility),
        "posting:default:sensitive": false,
        "posting:default:language": "en",
        "reading:expand:media": "default",
        "reading:expand:spoilers": false,
    }))
}

async fn mastodon_account(env: &Env) -> Result<Value> {
    let db = env.d1("DB")?;
    let actor_origin = format!("https://{}", activitypub_domain(env));
    let actor = db
        .prepare(
            "SELECT id, username, display_name, summary, avatar_url, header_url, created_at FROM actors WHERE username = 'social' LIMIT 1",
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let followers = db
        .prepare("SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'")
        .first::<Map<String, Value>>(None)
        .await?;
    let following = db
        .prepare("SELECT COUNT(*) AS count FROM following WHERE status = 'accepted'")
        .first::<Map<String, Value>>(None)
        .await?;
    let username = string_field(actor.as_ref(), "username").unwrap_or_else(|| "social".to_string());
    let actor_id = string_field(actor.as_ref(), "id")
        .unwrap_or_else(|| format!("{actor_origin}/users/{username}"));
    let display_name =
        string_field(actor.as_ref(), "display_name").unwrap_or_else(|| username.clone());
    let summary = string_field(actor.as_ref(), "summary").unwrap_or_default();
    let avatar = string_field(actor.as_ref(), "avatar_url").unwrap_or_default();
    let header = string_field(actor.as_ref(), "header_url").unwrap_or_default();
    let created_at = string_field(actor.as_ref(), "created_at")
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string());

    Ok(serde_json::json!({
        "id": actor_id,
        "username": username,
        "acct": username,
        "display_name": display_name,
        "locked": true,
        "bot": false,
        "discoverable": false,
        "group": false,
        "created_at": created_at,
        "note": summary,
        "url": format!("{actor_origin}/users/{username}"),
        "avatar": avatar,
        "avatar_static": avatar,
        "header": header,
        "header_static": header,
        "followers_count": integer_field(followers.as_ref(), "count"),
        "following_count": integer_field(following.as_ref(), "count"),
        "statuses_count": public_status_count(env).await?,
        "fields": [],
        "emojis": [],
    }))
}

async fn mastodon_update_credentials(env: &Env, body: &Value) -> Result<Response> {
    let mut profile = Map::new();
    if let Some(value) = body.get("display_name") {
        profile.insert("display_name".to_string(), value.clone());
    }
    if let Some(value) = body.get("note") {
        profile.insert("summary".to_string(), value.clone());
    }
    if !profile.is_empty() {
        if let Err(message) = owner_update_profile(env, &Value::Object(profile)).await {
            return api_json(&serde_json::json!({ "error": message }), 400);
        }
    }
    api_json(&mastodon_account(env).await?, 200)
}

async fn mastodon_statuses(env: &Env, limit: i32, url: &worker::Url) -> Result<Value> {
    let rows = mastodon_status_rows(env, "posts", limit, url).await?;
    mastodon_status_values(env, rows).await
}

async fn mastodon_statuses_by_interaction(
    env: &Env,
    interaction_type: &str,
    limit: i32,
    url: &worker::Url,
) -> Result<Value> {
    let db = env.d1("DB")?;
    let cursors = mastodon_cursor_options(url);
    let where_clause = mastodon_status_list_where("p", &cursors, 1);
    let mut args = vec![D1Type::Text(interaction_type)];
    if let Some(max_id) = cursors.max_id.as_deref() {
        args.push(D1Type::Text(max_id));
    }
    if let Some(newer_than) = cursors.newer_than.as_deref() {
        args.push(D1Type::Text(newer_than));
    }
    args.push(D1Type::Integer(limit));
    let limit_index = args.len();
    let query = format!(
        r#"
        SELECT p.id, p.actor_id, p.content, p.content_html, COALESCE(p.object_type, 'Note') AS object_type,
               p.name, p.summary, p.visibility, p.published_at, p.in_reply_to, p.poll_options, p.media_attachments,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = p.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions li WHERE (li.post_id = p.id OR li.object_url = p.id) AND li.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions bi WHERE (bi.post_id = p.id OR bi.object_url = p.id) AND bi.type = 'boost') AS boost_count,
               EXISTS(SELECT 1 FROM interactions oi WHERE (oi.post_id = p.id OR oi.object_url = p.id) AND oi.type = 'like' AND oi.actor_id = p.actor_id) AS favourited,
               EXISTS(SELECT 1 FROM interactions oi WHERE (oi.post_id = p.id OR oi.object_url = p.id) AND oi.type = 'boost' AND oi.actor_id = p.actor_id) AS reblogged
        FROM posts p
        JOIN interactions i ON i.object_url = p.id OR i.post_id = p.id
        WHERE i.type = ?1
          AND {where_clause}
        ORDER BY i.created_at DESC
        LIMIT ?{limit_index}
        "#,
    );
    let refs: Vec<&D1Type> = args.iter().collect();
    let rows = db
        .prepare(&query)
        .bind_refs(refs)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    mastodon_status_values(env, rows).await
}

async fn mastodon_status_rows(
    env: &Env,
    alias: &str,
    limit: i32,
    url: &worker::Url,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let cursors = mastodon_cursor_options(url);
    let where_clause = mastodon_status_list_where(alias, &cursors, 0);
    let mut args = Vec::new();
    if let Some(max_id) = cursors.max_id.as_deref() {
        args.push(D1Type::Text(max_id));
    }
    if let Some(newer_than) = cursors.newer_than.as_deref() {
        args.push(D1Type::Text(newer_than));
    }
    args.push(D1Type::Integer(limit));
    let limit_index = args.len();
    let query = format!(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
        FROM posts
        WHERE {where_clause}
        ORDER BY published_at DESC
        LIMIT ?{limit_index}
        "#,
    );
    let refs: Vec<&D1Type> = args.iter().collect();
    db.prepare(&query)
        .bind_refs(refs)?
        .all()
        .await?
        .results::<Map<String, Value>>()
}

async fn mastodon_status_values(env: &Env, rows: Vec<Map<String, Value>>) -> Result<Value> {
    let account = mastodon_account(env).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| mastodon_status_json(&row, &account))
            .collect(),
    ))
}

async fn mastodon_status(env: &Env, id: &str) -> Result<Option<Value>> {
    let Some(row) = mastodon_status_row(env, id).await? else {
        return Ok(None);
    };
    let account = mastodon_account(env).await?;
    Ok(Some(mastodon_status_json(&row, &account)))
}

async fn mastodon_status_row(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let canonical_id = canonical_mastodon_status_id(id);
    let id_arg = D1Type::Text(&canonical_id);
    db.prepare(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
        FROM posts
        WHERE id = ?1
          AND visibility = 'public'
          AND encrypted_message IS NULL
          AND content NOT LIKE '%End-to-end encrypted message%'
        LIMIT 1
        "#,
    )
    .bind_refs(&id_arg)?
    .first::<Map<String, Value>>(None)
    .await
}

async fn mastodon_status_context(env: &Env, id: &str) -> Result<Option<Value>> {
    let canonical_id = canonical_mastodon_status_id(id);
    let Some(status) = mastodon_status(env, &canonical_id).await? else {
        return Ok(None);
    };

    let mut ancestors = Vec::new();
    let mut seen = vec![canonical_id.clone()];
    let mut parent_id = status
        .get("in_reply_to_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    while let Some(parent) = parent_id {
        if ancestors.len() >= 20 || seen.iter().any(|value| value == &parent) {
            break;
        }
        seen.push(parent.clone());
        let Some(parent_status) = mastodon_status(env, &parent).await? else {
            break;
        };
        parent_id = parent_status
            .get("in_reply_to_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        ancestors.insert(0, parent_status);
    }

    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(&canonical_id);
    let rows = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
                   (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
                   EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
                   EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
            FROM posts
            WHERE in_reply_to = ?1
              AND visibility = 'public'
              AND encrypted_message IS NULL
              AND content NOT LIKE '%End-to-end encrypted message%'
            ORDER BY published_at ASC
            LIMIT 40
            "#,
        )
        .bind_refs(&id_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let account = mastodon_account(env).await?;
    Ok(Some(serde_json::json!({
        "ancestors": ancestors,
        "descendants": rows.into_iter().map(|row| mastodon_status_json(&row, &account)).collect::<Vec<_>>(),
    })))
}

fn mastodon_status_source_json(row: &Map<String, Value>) -> Value {
    serde_json::json!({
        "id": row_value_or_null(row, "id"),
        "text": mastodon_status_content(row),
        "spoiler_text": string_field(Some(row), "summary").unwrap_or_default(),
    })
}

async fn mastodon_create_status(env: &Env, body: &Value) -> Result<Response> {
    let text = body_string_any(body, &["status", "text"]).unwrap_or_default();
    if text.trim().is_empty() {
        return api_json(&serde_json::json!({ "error": "status is required" }), 400);
    }
    let visibility = normalize_mastodon_visibility(
        &body
            .get("visibility")
            .and_then(optional_body_string)
            .unwrap_or_else(|| "private".to_string()),
    )
    .unwrap_or_else(|| "followers".to_string());
    let poll = match mastodon_poll_from_body(body) {
        Ok(poll) => poll,
        Err(message) => return api_json(&serde_json::json!({ "error": message }), 400),
    };
    let media_ids = request_body_array(body, "media_ids");
    let attachments = match mastodon_attachments_for_media_ids(env, &media_ids, &visibility).await {
        Ok(attachments) => attachments,
        Err(message) => return api_json(&serde_json::json!({ "error": message }), 400),
    };
    let in_reply_to = body.get("in_reply_to_id").and_then(optional_body_string);
    let summary = body.get("spoiler_text").and_then(optional_body_string);
    let object_type = if poll.is_some() { "Question" } else { "Note" };
    match owner_create_post(
        env,
        text.trim(),
        &visibility,
        "activitypub",
        Vec::new(),
        attachments,
        false,
        in_reply_to.clone(),
        None,
        object_type,
        summary.clone(),
        poll.clone(),
    )
    .await
    {
        Ok(created) => {
            let account = mastodon_account(env).await?;
            api_json(
                &mastodon_status_json(&mastodon_created_status_row(created), &account),
                201,
            )
        }
        Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
    }
}

async fn mastodon_update_status(env: &Env, id: &str, body: &Value) -> Result<Response> {
    let canonical_id = canonical_mastodon_status_id(id);
    if mastodon_status(env, &canonical_id).await?.is_none() {
        return api_json(&serde_json::json!({ "error": "Record not found" }), 404);
    }
    let text = body_string_any(body, &["status", "text"]).unwrap_or_default();
    let summary = body.get("spoiler_text").and_then(optional_body_string);

    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(&canonical_id);
    if text.trim().is_empty() {
        let summary_arg = summary.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
        db.prepare("UPDATE posts SET updated_at = CURRENT_TIMESTAMP, summary = ?1 WHERE id = ?2")
            .bind_refs([&summary_arg, &id_arg])?
            .run()
            .await?;
    } else {
        let content_html = format!("<p>{}</p>", escape_html(text.trim()).replace('\n', "<br>"));
        let text_arg = D1Type::Text(text.trim());
        let html_arg = D1Type::Text(&content_html);
        let summary_arg = summary.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
        db.prepare(
            "UPDATE posts SET updated_at = CURRENT_TIMESTAMP, content = ?1, content_html = ?2, summary = ?3 WHERE id = ?4",
        )
        .bind_refs([&text_arg, &html_arg, &summary_arg, &id_arg])?
        .run()
        .await?;
    }

    match mastodon_status(env, &canonical_id).await? {
        Some(value) => api_json(&value, 200),
        None => api_json(&serde_json::json!({ "error": "Record not found" }), 404),
    }
}

async fn mastodon_delete_status(env: &Env, id: &str) -> Result<Response> {
    let canonical_id = canonical_mastodon_status_id(id);
    let Some(existing) = mastodon_status(env, &canonical_id).await? else {
        return api_json(&serde_json::json!({ "error": "Record not found" }), 404);
    };
    owner_delete_post(env, &canonical_id).await?;
    api_json(&existing, 200)
}

fn mastodon_created_status_row(created: Map<String, Value>) -> Map<String, Value> {
    let mut row = Map::new();
    for key in [
        "id",
        "actor_id",
        "content",
        "content_html",
        "object_type",
        "summary",
        "visibility",
        "published_at",
        "in_reply_to",
        "poll_options",
    ] {
        row.insert(key.to_string(), row_value_or_null(&created, key));
    }
    let attachments = created
        .get("attachments")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    row.insert(
        "media_attachments".to_string(),
        if attachments.as_array().map(Vec::is_empty).unwrap_or(true) {
            Value::Null
        } else {
            Value::String(attachments.to_string())
        },
    );
    row.insert("reply_count".to_string(), Value::from(0));
    row.insert("like_count".to_string(), Value::from(0));
    row.insert("boost_count".to_string(), Value::from(0));
    row.insert("favourited".to_string(), Value::Bool(false));
    row.insert("reblogged".to_string(), Value::Bool(false));
    row
}

fn normalize_mastodon_visibility(value: &str) -> Option<String> {
    match value.to_ascii_lowercase().as_str() {
        "public" => Some("public".to_string()),
        "unlisted" => Some("unlisted".to_string()),
        "private" | "followers" => Some("followers".to_string()),
        "direct" => Some("direct".to_string()),
        _ => None,
    }
}

fn mastodon_poll_from_body(body: &Value) -> std::result::Result<Option<Value>, String> {
    let options = mastodon_poll_options_from_body(body);
    let multiple = mastodon_poll_multiple_from_body(body);
    if options.is_empty() {
        if multiple {
            return Err("poll[multiple] requires poll[options][]".to_string());
        }
        return Ok(None);
    }
    if options.len() < 2 || options.len() > 4 {
        return Err("polls require between two and four options".to_string());
    }
    for option in &options {
        if option.trim().is_empty() {
            return Err("poll options must not be empty".to_string());
        }
        if option.chars().count() > 200 {
            return Err("poll options must be 200 characters or fewer".to_string());
        }
    }
    Ok(Some(
        serde_json::json!({ "multiple": multiple, "options": options }),
    ))
}

fn mastodon_poll_options_from_body(body: &Value) -> Vec<String> {
    let poll = body.get("poll").and_then(Value::as_object);
    let candidates = [
        poll.and_then(|poll| poll.get("options")),
        body.get("poll[options]"),
        body.get("poll[options][]"),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(|value| {
            let values = array_from_body_value(value)
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(values)
        })
        .unwrap_or_default()
}

fn mastodon_poll_multiple_from_body(body: &Value) -> bool {
    let poll = body.get("poll").and_then(Value::as_object);
    let value = poll
        .and_then(|poll| poll.get("multiple"))
        .or_else(|| body.get("poll[multiple]"));
    value
        .map(|value| {
            matches!(
                value
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str(),
                "true" | "1" | "on" | "yes"
            ) || matches!(value, Value::Bool(true))
        })
        .unwrap_or(false)
}

async fn mastodon_attachments_for_media_ids(
    env: &Env,
    media_ids: &[String],
    visibility: &str,
) -> std::result::Result<Vec<Value>, String> {
    let mut attachments = Vec::new();
    for id in media_ids {
        let url = id.trim();
        if url.is_empty() {
            continue;
        }
        let attachment = serde_json::json!({ "url": url });
        if matches!(visibility, "followers" | "direct") && !is_private_media_attachment(&attachment)
        {
            return Err("Mastodon API private media posts require private media URLs".to_string());
        }
        let media_attachment = mastodon_media_attachment_for_id(env, url)
            .await
            .map_err(|error| error.to_string())?;
        attachments.push(serde_json::json!({
            "type": "Document",
            "url": url,
            "mediaType": media_attachment
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|attachment| string_field(Some(attachment), "media_type"))
                .unwrap_or_else(|| media_type_for_filename(url)),
            "name": media_attachment
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|attachment| string_field(Some(attachment), "description"))
                .unwrap_or_else(|| decode_component(url.rsplit('/').next().unwrap_or("media"))),
        }));
    }
    Ok(attachments)
}

async fn mastodon_upload_media(env: &Env, body: &Value) -> Result<Response> {
    let data_base64 = body.get("data_base64").and_then(optional_body_string);
    let Some(data_base64) = data_base64 else {
        return api_json(&serde_json::json!({ "error": "file is required" }), 400);
    };
    let filename = body_string_any(body, &["filename", "description"])
        .unwrap_or_else(|| "upload.bin".to_string());
    let media_type = body_string_any(body, &["media_type", "content_type"])
        .unwrap_or_else(|| media_type_for_filename(&filename));
    let description = body.get("description").and_then(optional_body_string);

    let mut upload = Map::new();
    upload.insert("filename".to_string(), Value::String(filename));
    upload.insert("data_base64".to_string(), Value::String(data_base64));
    upload.insert("media_type".to_string(), Value::String(media_type));
    upload.insert("access".to_string(), Value::String("public".to_string()));
    upload.insert(
        "description".to_string(),
        description
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );

    match owner_upload_media(env, &Value::Object(upload)).await {
        Ok(uploaded) => api_json(
            &mastodon_media_attachment_from_upload(&uploaded, description),
            200,
        ),
        Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
    }
}

async fn mastodon_upload_media_multipart(env: &Env, req: &mut Request) -> Result<Response> {
    let form = req.form_data().await?;
    let description = form_field(&form, "description");
    let mut filename = form_field(&form, "filename")
        .or_else(|| description.clone())
        .unwrap_or_else(|| "upload.bin".to_string());
    let mut media_type = form_field(&form, "media_type")
        .or_else(|| form_field(&form, "content_type"))
        .unwrap_or_default();
    let mut data_base64 = form_field(&form, "data_base64");

    let file = form.get("file").or_else(|| form.get("file[]"));
    if let Some(FormEntry::File(file)) = file {
        let file_name = file.name();
        if !file_name.trim().is_empty() {
            filename = file_name;
        }
        let file_type = file.type_();
        if !file_type.trim().is_empty() {
            media_type = file_type;
        } else if media_type.is_empty() {
            media_type = media_type_for_filename(&filename);
        }
        data_base64 = Some(BASE64.encode(file.bytes().await?));
    }

    let Some(data_base64) = data_base64 else {
        return api_json(&serde_json::json!({ "error": "file is required" }), 400);
    };
    if media_type.is_empty() {
        media_type = media_type_for_filename(&filename);
    }

    let mut upload = Map::new();
    upload.insert("filename".to_string(), Value::String(filename));
    upload.insert("data_base64".to_string(), Value::String(data_base64));
    upload.insert("media_type".to_string(), Value::String(media_type));
    upload.insert("access".to_string(), Value::String("public".to_string()));
    upload.insert(
        "description".to_string(),
        description
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );

    match owner_upload_media(env, &Value::Object(upload)).await {
        Ok(uploaded) => api_json(
            &mastodon_media_attachment_from_upload(&uploaded, description),
            200,
        ),
        Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
    }
}

fn form_field(form: &FormData, name: &str) -> Option<String> {
    form.get_field(name).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn mastodon_media_attachment_from_upload(
    uploaded: &Map<String, Value>,
    description: Option<String>,
) -> Value {
    let attachment = uploaded.get("attachment").and_then(Value::as_object);
    let url = string_field(Some(uploaded), "url")
        .or_else(|| string_field(attachment, "url"))
        .unwrap_or_default();
    let media_type = string_field(Some(uploaded), "media_type")
        .or_else(|| string_field(attachment, "mediaType"))
        .unwrap_or_default();
    serde_json::json!({
        "id": url,
        "type": mastodon_media_attachment_type(&media_type),
        "media_type": media_type,
        "url": url,
        "preview_url": url,
        "remote_url": Value::Null,
        "preview_remote_url": Value::Null,
        "text_url": Value::Null,
        "meta": {},
        "description": description
            .or_else(|| string_field(Some(uploaded), "description"))
            .or_else(|| string_field(attachment, "name"))
            .map(Value::String)
            .unwrap_or(Value::Null),
        "blurhash": Value::Null,
    })
}

async fn mastodon_media_attachment_for_id(env: &Env, id: &str) -> Result<Option<Value>> {
    let Some(key) = mastodon_media_r2_key(env, id) else {
        return Ok(None);
    };
    let bucket = env.bucket("MEDIA_BUCKET")?;
    let Some(object) = bucket.get(key.clone()).execute().await? else {
        return Ok(None);
    };
    let metadata = object.http_metadata();
    let media_type = metadata
        .content_type
        .unwrap_or_else(|| media_type_for_filename(&key));
    let custom_metadata = object.custom_metadata().unwrap_or_default();
    let description = custom_metadata
        .get("description")
        .cloned()
        .unwrap_or_else(|| decode_component(key.rsplit('/').next().unwrap_or("media")));
    Ok(Some(mastodon_media_attachment_for_key(
        env,
        &key,
        &media_type,
        &description,
    )))
}

async fn mastodon_update_media_attachment(
    env: &Env,
    id: &str,
    description: Option<String>,
) -> Result<Option<Value>> {
    let Some(key) = mastodon_media_r2_key(env, id) else {
        return Ok(None);
    };
    let bucket = env.bucket("MEDIA_BUCKET")?;
    let Some(object) = bucket.get(key.clone()).execute().await? else {
        return Ok(None);
    };
    let bytes = match object.body() {
        Some(body) => body.bytes().await?,
        None => Vec::new(),
    };
    let metadata = object.http_metadata();
    let media_type = metadata
        .content_type
        .clone()
        .unwrap_or_else(|| media_type_for_filename(&key));
    let mut custom_metadata = object.custom_metadata().unwrap_or_default();
    if let Some(description) = description.as_deref() {
        custom_metadata.insert("description".to_string(), description.to_string());
    } else {
        custom_metadata.remove("description");
    }
    let mut http_metadata = worker::HttpMetadata::default();
    http_metadata.content_type = Some(media_type.clone());
    bucket
        .put(key.clone(), bytes)
        .http_metadata(http_metadata)
        .custom_metadata(custom_metadata)
        .execute()
        .await?;
    mastodon_media_attachment_for_id(env, id).await
}

fn mastodon_media_attachment_for_key(
    env: &Env,
    key: &str,
    media_type: &str,
    description: &str,
) -> Value {
    let url = format!("https://{}/media/{key}", activitypub_domain(env));
    serde_json::json!({
        "id": url,
        "type": mastodon_media_attachment_type(media_type),
        "url": url,
        "preview_url": url,
        "remote_url": Value::Null,
        "preview_remote_url": Value::Null,
        "text_url": Value::Null,
        "meta": {},
        "description": description,
        "blurhash": Value::Null,
    })
}

fn mastodon_media_attachment_type(media_type: &str) -> &'static str {
    if media_type.starts_with("image/") {
        "image"
    } else if media_type.starts_with("video/") {
        "video"
    } else {
        "unknown"
    }
}

fn mastodon_media_r2_key(env: &Env, id: &str) -> Option<String> {
    let parsed = worker::Url::parse(id).ok()?;
    if parsed.host_str()? != activitypub_domain(env) {
        return None;
    }
    let rest = parsed.path().strip_prefix("/media/uploads/")?;
    (!rest.is_empty()).then(|| decode_component(&format!("uploads/{rest}")))
}

fn array_from_body_value(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(optional_body_string).collect(),
        Value::Null => Vec::new(),
        value => optional_body_string(value).into_iter().collect(),
    }
}

async fn mastodon_status_action(env: &Env, status_id: &str, action: &str) -> Result<Response> {
    let canonical_id = canonical_mastodon_status_id(status_id);
    let Some(existing) = mastodon_status(env, &canonical_id).await? else {
        return api_json(&serde_json::json!({ "error": "Record not found" }), 404);
    };
    mastodon_toggle_status_interaction(env, &canonical_id, action).await?;
    let value = mastodon_status(env, &canonical_id)
        .await?
        .unwrap_or(existing);
    api_json(&value, 200)
}

async fn mastodon_toggle_status_interaction(
    env: &Env,
    status_id: &str,
    action: &str,
) -> Result<()> {
    let local_actor = owner_local_actor(env).await?;
    let interaction_type = match action {
        "favourite" | "unfavourite" => "like",
        "reblog" | "unreblog" => "boost",
        _ => return Ok(()),
    };
    let suffix: String = stable_id(status_id).chars().take(16).collect();
    let interaction_id = format!("{}#{}s/{}", local_actor.id, interaction_type, suffix);
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(&interaction_id);

    if matches!(action, "unfavourite" | "unreblog") {
        db.prepare("DELETE FROM interactions WHERE id = ?1")
            .bind_refs(&id_arg)?
            .run()
            .await?;
        return Ok(());
    }

    let type_arg = D1Type::Text(interaction_type);
    let actor_arg = D1Type::Text(&local_actor.id);
    let object_arg = D1Type::Text(status_id);
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let now_arg = D1Type::Text(&now);
    db.prepare(
        r#"
        INSERT OR REPLACE INTO interactions (
          id, type, actor_id, object_url, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind_refs([&id_arg, &type_arg, &actor_arg, &object_arg, &now_arg])?
    .run()
    .await?;
    Ok(())
}

async fn mastodon_clear_notifications(env: &Env) -> Result<Response> {
    env.d1("DB")?
        .prepare("UPDATE notifications SET read = 1")
        .run()
        .await?;
    api_json(&serde_json::json!({}), 200)
}

async fn mastodon_dismiss_notification(env: &Env, id: &str) -> Result<Response> {
    let id_arg = D1Type::Text(id);
    env.d1("DB")?
        .prepare("UPDATE notifications SET read = 1 WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    api_json(&serde_json::json!({}), 200)
}

async fn mastodon_blocks(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT actor_id, actor_id AS url, created_at
            FROM blocks
            WHERE actor_id IS NOT NULL AND actor_id != ''
              AND actor_id NOT LIKE 'domain:%'
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(rows.iter().map(remote_account_json).collect()))
}

async fn mastodon_domain_blocks(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT blocked_domain
            FROM blocks
            WHERE blocked_domain IS NOT NULL AND blocked_domain != ''
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(
        rows.iter()
            .filter_map(|row| string_field(Some(row), "blocked_domain").map(Value::String))
            .collect(),
    ))
}

async fn mastodon_set_domain_block(
    env: &Env,
    body: &Value,
    url: &worker::Url,
    enabled: bool,
) -> Result<Response> {
    let domain = body
        .get("domain")
        .and_then(optional_body_string)
        .or_else(|| query_param(url, "domain"))
        .unwrap_or_default();
    let domain = match normalize_host_value(&domain) {
        Ok(domain) => domain,
        Err(_) => return api_json(&serde_json::json!({ "error": "domain is required" }), 400),
    };

    if enabled {
        let id = format!("domain-block-{}", stable_id(&domain));
        insert_block(
            env,
            &id,
            &format!("domain:{domain}"),
            Some(&domain),
            Some("Mastodon API domain block"),
        )
        .await
        .map_err(worker::Error::RustError)?;
    } else {
        let db = env.d1("DB")?;
        let domain_arg = D1Type::Text(&domain);
        db.prepare("DELETE FROM blocks WHERE blocked_domain = ?1")
            .bind_refs(&domain_arg)?
            .run()
            .await?;
    }

    api_json(&serde_json::json!({}), 200)
}

async fn mastodon_account_action(env: &Env, id: &str, action: &str) -> Result<Response> {
    match action {
        "follow" => {
            if let Err(message) = owner_follow_actor(env, id).await {
                return api_json(&serde_json::json!({ "error": message }), 400);
            }
        }
        "unfollow" => {
            let _ = owner_unfollow_actor(env, id).await;
        }
        "block" => {
            mastodon_set_account_block(env, id, true).await?;
        }
        "unblock" => {
            mastodon_set_account_block(env, id, false).await?;
        }
        "mute" | "unmute" => {}
        _ => {}
    }
    let relationship = mastodon_relationship(env, id).await?;
    api_json(&relationship, 200)
}

async fn mastodon_set_account_block(env: &Env, actor_id: &str, enabled: bool) -> Result<()> {
    let db = env.d1("DB")?;
    let actor_arg = D1Type::Text(actor_id);
    if enabled {
        let id = format!("block-{}", stable_id(actor_id));
        let id_arg = D1Type::Text(&id);
        db.prepare(
            r#"
            INSERT OR REPLACE INTO blocks (id, actor_id, reason, created_at)
            VALUES (?1, ?2, 'Mastodon API block', CURRENT_TIMESTAMP)
            "#,
        )
        .bind_refs([&id_arg, &actor_arg])?
        .run()
        .await?;
    } else {
        db.prepare("DELETE FROM blocks WHERE actor_id = ?1")
            .bind_refs(&actor_arg)?
            .run()
            .await?;
    }
    Ok(())
}

async fn mastodon_search(env: &Env, query: &str, limit: i32, url: &worker::Url) -> Result<Value> {
    let term = query.trim();
    if term.is_empty() {
        return Ok(serde_json::json!({ "accounts": [], "statuses": [], "hashtags": [] }));
    }

    if term.starts_with('@') || term.starts_with("https://") {
        if let Ok(actor) = owner_discover_actor(env, term).await {
            let mut row = Map::new();
            if let Some(id) = actor.get("id").and_then(Value::as_str) {
                row.insert("actor_id".to_string(), Value::String(id.to_string()));
                row.insert("url".to_string(), Value::String(id.to_string()));
                row.insert(
                    "created_at".to_string(),
                    Value::String("1970-01-01T00:00:00.000Z".to_string()),
                );
                return Ok(serde_json::json!({
                    "accounts": [remote_account_json(&row)],
                    "statuses": [],
                    "hashtags": [],
                }));
            }
        }
    }

    let db = env.d1("DB")?;
    let cursors = mastodon_cursor_options(url);
    let where_clause = mastodon_status_list_where("posts", &cursors, 0);
    let like = format!("%{term}%");
    let mut args = Vec::new();
    if let Some(max_id) = cursors.max_id.as_deref() {
        args.push(D1Type::Text(max_id));
    }
    if let Some(newer_than) = cursors.newer_than.as_deref() {
        args.push(D1Type::Text(newer_than));
    }
    args.push(D1Type::Text(&like));
    let term_index = args.len();
    args.push(D1Type::Integer(limit));
    let limit_index = args.len();
    let query = format!(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, visibility, published_at, in_reply_to, poll_options, media_attachments,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like' AND i.actor_id = posts.actor_id) AS favourited,
               EXISTS(SELECT 1 FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost' AND i.actor_id = posts.actor_id) AS reblogged
        FROM posts
        WHERE {where_clause}
          AND (content LIKE ?{term_index} OR name LIKE ?{term_index} OR summary LIKE ?{term_index})
        ORDER BY published_at DESC
        LIMIT ?{limit_index}
        "#,
    );
    let refs: Vec<&D1Type> = args.iter().collect();
    let rows = db
        .prepare(&query)
        .bind_refs(refs)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let statuses = mastodon_status_values(env, rows).await?;
    Ok(serde_json::json!({
        "accounts": [],
        "statuses": statuses,
        "hashtags": [],
    }))
}

async fn mastodon_conversations(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, participants, last_message_at
            FROM conversations
            ORDER BY last_message_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let accounts = parse_json_array(row.get("participants"))
                    .into_iter()
                    .filter_map(|actor_id| actor_id.as_str().map(ToOwned::to_owned))
                    .map(|actor_id| {
                        let mut account = Map::new();
                        account.insert("actor_id".to_string(), Value::String(actor_id.clone()));
                        account.insert("url".to_string(), Value::String(actor_id));
                        remote_account_json(&account)
                    })
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "id": row_value_or_null(&row, "id"),
                    "unread": false,
                    "last_status": Value::Null,
                    "accounts": accounts,
                })
            })
            .collect(),
    ))
}

fn parse_json_array(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(text)) => serde_json::from_str::<Vec<Value>>(text).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn mastodon_streaming_response() -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "text/event-stream")?;
    headers.set("Cache-Control", "no-cache")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("X-Accel-Buffering", "no")?;
    Ok(Response::ok(
        "retry: 30000\nevent: connected\ndata: {\"stream\":\"polling-recommended\"}\n\n",
    )?
    .with_headers(headers))
}

fn mastodon_report(body: &Value) -> Value {
    let account_id = body.get("account_id").and_then(optional_body_string);
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let id = format!(
        "report-{}",
        stable_id(&format!(
            "{}\n{}",
            account_id.as_deref().unwrap_or_default(),
            js_sys::Date::now()
        ))
    );
    let status_ids = request_body_array(body, "status_ids");
    serde_json::json!({
        "id": id,
        "action_taken": false,
        "action_taken_at": Value::Null,
        "category": body.get("category").and_then(optional_body_string).unwrap_or_else(|| "other".to_string()),
        "comment": body.get("comment").and_then(optional_body_string).unwrap_or_default(),
        "forwarded": false,
        "created_at": now,
        "status_ids": status_ids,
        "rules": [],
        "target_account": account_id.map(|id| {
            let mut row = Map::new();
            row.insert("actor_id".to_string(), Value::String(id.clone()));
            row.insert("url".to_string(), Value::String(id));
            remote_account_json(&row)
        }).unwrap_or(Value::Null),
    })
}

fn request_body_array(body: &Value, key: &str) -> Vec<String> {
    let bracket = format!("{key}[]");
    let value = body.get(&bracket).or_else(|| body.get(key));
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(optional_body_string)
            .collect::<Vec<_>>(),
        Some(value) => optional_body_string(value).into_iter().collect(),
        None => Vec::new(),
    }
}

struct MastodonCursorOptions {
    max_id: Option<String>,
    newer_than: Option<String>,
}

fn mastodon_cursor_options(url: &worker::Url) -> MastodonCursorOptions {
    MastodonCursorOptions {
        max_id: query_param(url, "max_id").filter(|value| !value.trim().is_empty()),
        newer_than: query_param(url, "since_id")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| query_param(url, "min_id").filter(|value| !value.trim().is_empty())),
    }
}

fn mastodon_status_list_where(
    alias: &str,
    cursors: &MastodonCursorOptions,
    placeholder_offset: usize,
) -> String {
    let mut conditions = vec![
        format!("{alias}.visibility = 'public'"),
        format!("{alias}.encrypted_message IS NULL"),
        format!("{alias}.content NOT LIKE '%End-to-end encrypted message%'"),
    ];
    let mut index = placeholder_offset;
    if cursors.max_id.is_some() {
        index += 1;
        conditions.push(format!("{alias}.id < ?{index}"));
    }
    if cursors.newer_than.is_some() {
        index += 1;
        conditions.push(format!("{alias}.id > ?{index}"));
    }
    conditions.join("\n       AND ")
}

fn mastodon_status_json(row: &Map<String, Value>, account: &Value) -> Value {
    serde_json::json!({
        "id": row_value_or_null(row, "id"),
        "uri": row_value_or_null(row, "id"),
        "url": row_value_or_null(row, "id"),
        "account": account,
        "in_reply_to_id": row_value_or_null(row, "in_reply_to"),
        "in_reply_to_account_id": Value::Null,
        "reblog": Value::Null,
        "content": mastodon_status_content(row),
        "plain_text": mastodon_plain_text(row),
        "created_at": row_value_or_null(row, "published_at"),
        "edited_at": Value::Null,
        "emojis": [],
        "replies_count": integer_field(Some(row), "reply_count"),
        "reblogs_count": integer_field(Some(row), "boost_count"),
        "favourites_count": integer_field(Some(row), "like_count"),
        "reblogged": bool_field(Some(row), "reblogged"),
        "favourited": bool_field(Some(row), "favourited"),
        "muted": false,
        "sensitive": false,
        "spoiler_text": "",
        "visibility": mastodon_visibility(&string_field(Some(row), "visibility").unwrap_or_default()),
        "media_attachments": mastodon_media_attachments(row),
        "mentions": mastodon_mentions(row),
        "tags": mastodon_tags(row),
        "card": Value::Null,
        "poll": mastodon_poll_json(row),
    })
}

fn mastodon_plain_text(row: &Map<String, Value>) -> String {
    ["name", "summary", "content"]
        .iter()
        .filter_map(|key| string_field(Some(row), key))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn mastodon_status_content(row: &Map<String, Value>) -> String {
    let mut parts = Vec::new();
    if let Some(name) = string_field(Some(row), "name") {
        parts.push(format!("<p><strong>{}</strong></p>", escape_html(&name)));
    }
    if let Some(summary) = string_field(Some(row), "summary") {
        parts.push(format!("<p>{}</p>", escape_html(&summary)));
    }
    parts.push(
        string_field(Some(row), "content_html").unwrap_or_else(|| {
            escape_html(&string_field(Some(row), "content").unwrap_or_default())
        }),
    );
    parts.join("")
}

fn mastodon_poll_json(row: &Map<String, Value>) -> Value {
    if string_field(Some(row), "object_type").as_deref() != Some("Question") {
        return Value::Null;
    }
    let Some(raw) = row.get("poll_options") else {
        return Value::Null;
    };
    let parsed = match raw {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        value => Some(value.clone()),
    };
    let Some(parsed) = parsed else {
        return Value::Null;
    };
    let options = parsed
        .get("options")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if options.is_empty() {
        return Value::Null;
    }
    serde_json::json!({
        "id": format!("{}#poll", string_field(Some(row), "id").unwrap_or_default()),
        "expires_at": Value::Null,
        "expired": false,
        "multiple": parsed.get("multiple").and_then(Value::as_bool).unwrap_or(false),
        "votes_count": 0,
        "voters_count": 0,
        "voted": false,
        "own_votes": [],
        "options": options.into_iter().map(|title| serde_json::json!({ "title": title, "votes_count": 0 })).collect::<Vec<_>>(),
        "emojis": [],
    })
}

fn mastodon_media_attachments(row: &Map<String, Value>) -> Value {
    Value::Array(
        parse_attachment_array(row.get("media_attachments"))
            .into_iter()
            .enumerate()
            .filter_map(|(index, attachment)| {
                let object = attachment.as_object()?;
                let url = string_field(Some(object), "url").unwrap_or_default();
                if url.is_empty() {
                    return None;
                }
                let media_type = string_field(Some(object), "mediaType").unwrap_or_default();
                let attachment_type = if media_type.starts_with("image/") {
                    "image"
                } else if media_type.starts_with("video/") {
                    "video"
                } else {
                    "unknown"
                };
                Some(serde_json::json!({
                    "id": format!("{}#media-{}", string_field(Some(row), "id").unwrap_or_default(), index + 1),
                    "type": attachment_type,
                    "url": url,
                    "preview_url": url,
                    "remote_url": Value::Null,
                    "preview_remote_url": Value::Null,
                    "text_url": Value::Null,
                    "meta": {},
                    "description": string_field(Some(object), "name").map(Value::String).unwrap_or(Value::Null),
                    "blurhash": Value::Null,
                }))
            })
            .collect(),
    )
}

fn mastodon_mentions(row: &Map<String, Value>) -> Value {
    let mut seen = Vec::new();
    let mut mentions = Vec::new();
    for token in mastodon_plain_text(row).split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | ',' | '.' | ':' | ';' | '!' | '?'
            )
        });
        let Some(rest) = trimmed.strip_prefix('@') else {
            continue;
        };
        let Some((username, host)) = rest.split_once('@') else {
            continue;
        };
        if username.is_empty() || !host.contains('.') {
            continue;
        }
        let host = host.to_ascii_lowercase();
        let acct = format!("{username}@{host}");
        if seen.iter().any(|value: &String| value == &acct) {
            continue;
        }
        seen.push(acct.clone());
        mentions.push(serde_json::json!({
            "id": format!("https://{host}/@{username}"),
            "username": username,
            "acct": acct,
            "url": format!("https://{host}/@{username}"),
        }));
    }
    Value::Array(mentions)
}

fn mastodon_tags(row: &Map<String, Value>) -> Value {
    let mut seen = Vec::new();
    let mut tags = Vec::new();
    for token in mastodon_plain_text(row).split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | ',' | '.' | ':' | ';' | '!' | '?'
            )
        });
        let Some(name) = trimmed.strip_prefix('#') else {
            continue;
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
        {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if seen.iter().any(|value: &String| value == &key) {
            continue;
        }
        seen.push(key);
        tags.push(serde_json::json!({
            "name": name,
            "url": format!("https://social.dais.social/tags/{name}"),
        }));
    }
    Value::Array(tags)
}

async fn mastodon_relationships(env: &Env, url: &worker::Url) -> Result<Value> {
    let mut ids = Vec::new();
    for (key, value) in url.query_pairs() {
        if key == "id[]" || key == "id" {
            let value = value.trim();
            if !value.is_empty() && !ids.iter().any(|existing| existing == value) {
                ids.push(value.to_string());
            }
        }
    }
    let mut relationships = Vec::new();
    for id in ids {
        relationships.push(mastodon_relationship(env, &id).await?);
    }
    Ok(Value::Array(relationships))
}

async fn mastodon_relationship(env: &Env, id: &str) -> Result<Value> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    let following = db
        .prepare("SELECT status FROM following WHERE target_actor_id = ?1 LIMIT 1")
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    let followed_by = db
        .prepare("SELECT status FROM followers WHERE follower_actor_id = ?1 LIMIT 1")
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    let blocked = db
        .prepare("SELECT 1 FROM blocks WHERE actor_id = ?1 OR blocked_domain = ?1 LIMIT 1")
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?
        .is_some();
    Ok(serde_json::json!({
        "id": id,
        "following": string_field(following.as_ref(), "status").as_deref() == Some("accepted"),
        "showing_reblogs": true,
        "notifying": false,
        "followed_by": string_field(followed_by.as_ref(), "status").as_deref() == Some("approved"),
        "blocking": blocked,
        "blocked_by": false,
        "muting": false,
        "muting_notifications": false,
        "requested": string_field(following.as_ref(), "status").as_deref() == Some("pending"),
        "domain_blocking": false,
        "endorsed": false,
        "note": "",
    }))
}

async fn mastodon_notifications(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, type, actor_id, actor_username, actor_display_name, content, post_id, created_at
            FROM notifications
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| {
                let actor_id = string_field(Some(&row), "actor_id").unwrap_or_default();
                let username =
                    string_field(Some(&row), "actor_username").unwrap_or_else(|| actor_id.clone());
                let display_name = string_field(Some(&row), "actor_display_name")
                    .unwrap_or_else(|| username.clone());
                let status = string_field(Some(&row), "post_id")
                    .map(|id| serde_json::json!({ "id": id, "uri": id, "url": id }))
                    .unwrap_or(Value::Null);
                serde_json::json!({
                    "id": row_value_or_null(&row, "id"),
                    "type": mastodon_notification_type(string_field(Some(&row), "type").as_deref()),
                    "created_at": row_value_or_null(&row, "created_at"),
                    "account": {
                        "id": actor_id,
                        "username": username,
                        "acct": username,
                        "display_name": display_name,
                        "url": actor_id,
                        "avatar": "",
                        "avatar_static": "",
                        "header": "",
                        "header_static": "",
                        "locked": false,
                        "bot": false,
                        "fields": [],
                        "emojis": [],
                    },
                    "status": status,
                })
            })
            .collect(),
    ))
}

fn mastodon_notification_type(value: Option<&str>) -> String {
    match value {
        Some("like") => "favourite".to_string(),
        Some("boost") => "reblog".to_string(),
        Some(value) if !value.is_empty() => value.to_string(),
        _ => "mention".to_string(),
    }
}

async fn mastodon_followers(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT follower_actor_id AS actor_id, follower_actor_id AS url, status, created_at
            FROM followers
            WHERE status = 'approved'
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(rows.iter().map(remote_account_json).collect()))
}

async fn mastodon_following(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT target_actor_id AS actor_id, target_actor_id AS url, status, created_at
            FROM following
            WHERE status = 'accepted'
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(Value::Array(rows.iter().map(remote_account_json).collect()))
}

fn remote_account_json(row: &Map<String, Value>) -> Value {
    let url = string_field(Some(row), "url")
        .or_else(|| string_field(Some(row), "actor_id"))
        .unwrap_or_default();
    let (username, acct) = parse_actor_acct(&url);
    serde_json::json!({
        "id": url,
        "username": username,
        "acct": acct,
        "display_name": username,
        "locked": false,
        "bot": false,
        "discoverable": false,
        "group": false,
        "created_at": string_field(Some(row), "created_at").unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string()),
        "note": "",
        "url": url,
        "avatar": "",
        "avatar_static": "",
        "header": "",
        "header_static": "",
        "followers_count": 0,
        "following_count": 0,
        "statuses_count": 0,
        "fields": [],
        "emojis": [],
    })
}

fn parse_actor_acct(actor_url: &str) -> (String, String) {
    match worker::Url::parse(actor_url) {
        Ok(url) => {
            let username = url
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(ToOwned::to_owned))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| url.host_str().unwrap_or(actor_url).to_string());
            let host = url.host_str().unwrap_or_default();
            (username.clone(), format!("{username}@{host}"))
        }
        Err(_) => (actor_url.to_string(), actor_url.to_string()),
    }
}

fn mastodon_visibility(value: &str) -> &'static str {
    match value {
        "public" => "public",
        "unlisted" => "unlisted",
        "direct" => "direct",
        _ => "private",
    }
}

fn mastodon_follow_request_action(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/follow_requests/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && matches!(parts.next(), Some("authorize" | "reject")) && parts.next().is_none()
}

fn mastodon_suggestion_dismiss(path: &str) -> bool {
    path.strip_prefix("/api/v1/suggestions/")
        .map(|rest| !rest.is_empty() && !rest.contains('/'))
        .unwrap_or(false)
}

fn mastodon_account_statuses_path(path: &str) -> bool {
    mastodon_account_collection_path(path, "statuses")
}

fn mastodon_account_followers_path(path: &str) -> bool {
    mastodon_account_collection_path(path, "followers")
}

fn mastodon_account_following_path(path: &str) -> bool {
    mastodon_account_collection_path(path, "following")
}

fn mastodon_account_collection_path(path: &str, collection: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && parts.next() == Some(collection) && parts.next().is_none()
}

fn mastodon_account_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    !rest.is_empty() && !rest.contains('/')
}

fn mastodon_account_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/accounts/")?;
    let mut parts = rest.split('/');
    let id = parts.next()?;
    let action = parts.next()?;
    if id.is_empty()
        || parts.next().is_some()
        || !matches!(
            action,
            "follow" | "unfollow" | "block" | "unblock" | "mute" | "unmute"
        )
    {
        return None;
    }
    Some((id.to_string(), action.to_string()))
}

fn mastodon_status_context_path(path: &str) -> Option<String> {
    mastodon_status_subpath(path, "context")
}

fn mastodon_status_source_path(path: &str) -> Option<String> {
    mastodon_status_subpath(path, "source")
}

fn mastodon_status_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    for action in ["favourite", "unfavourite", "reblog", "unreblog"] {
        let suffix = format!("/{action}");
        if let Some(id) = rest.strip_suffix(&suffix).filter(|id| !id.is_empty()) {
            return Some((id.to_string(), action.to_string()));
        }
    }
    None
}

fn mastodon_status_subpath(path: &str, suffix: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    let needle = format!("/{suffix}");
    let id = rest.strip_suffix(&needle)?;
    (!id.is_empty()).then(|| id.to_string())
}

fn mastodon_status_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    (!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
}

fn mastodon_media_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/v1/media/")
        .or_else(|| path.strip_prefix("/api/v2/media/"))
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

fn mastodon_notification_dismiss_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/notifications/")?;
    let id = rest.strip_suffix("/dismiss")?;
    (!id.is_empty()).then(|| id.to_string())
}

async fn public_status_count(env: &Env) -> Result<i64> {
    let row = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT COUNT(*) AS count
            FROM posts
            WHERE visibility = 'public'
              AND encrypted_message IS NULL
              AND content NOT LIKE '%End-to-end encrypted message%'
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(row
        .as_ref()
        .map(|fields| integer_field(Some(fields), "count"))
        .unwrap_or(0))
}

fn origin(url: &worker::Url) -> String {
    format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default())
}

fn owner_instance_url(env: &Env) -> String {
    let domain = env
        .var("DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());
    if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", domain.trim_end_matches('/'))
    }
}

async fn column_exists(env: &Env, table: &str, column: &str) -> Result<bool> {
    let db = env.d1("DB")?;
    let sql = format!("PRAGMA table_info({table})");
    let rows = db
        .prepare(&sql)
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows
        .iter()
        .filter_map(|row| string_field(Some(row), "name"))
        .any(|name| name == column))
}

async fn handle_owner_api(mut req: Request, env: Env, url: &worker::Url) -> Result<Response> {
    if req.method() == worker::Method::Options {
        return api_json(&serde_json::json!({}), 204);
    }

    let path = url.path();
    let owner_path = path
        .strip_prefix("/api/dais/owner")
        .filter(|value| !value.is_empty())
        .unwrap_or("/");
    let limit = clamp_limit(query_param(url, "limit"));
    if let Some(response) = require_owner_bearer(
        &req,
        &env,
        owner_api_required_scopes(req.method(), owner_path),
    )? {
        return Ok(response);
    }

    match (req.method(), owner_path) {
        (worker::Method::Get, "/snapshot") => api_json(&owner_snapshot(&env).await?, 200),
        (worker::Method::Get, "/settings") => api_json(&owner_snapshot_settings(&env).await?, 200),
        (worker::Method::Post, "/settings") => {
            let body = read_json(&mut req).await;
            match owner_update_settings(&env, &body).await {
                Ok(settings) => api_json(&settings, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/profile") => api_json(&owner_profile(&env).await?, 200),
        (worker::Method::Post, "/profile") => {
            let body = read_json(&mut req).await;
            match owner_update_profile(&env, &body).await {
                Ok(profile) => api_json(&profile, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/media") => {
            let body = read_json(&mut req).await;
            match owner_upload_media(&env, &body).await {
                Ok(result) => api_json(&result, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/media/revoke") => {
            let body = read_json(&mut req).await;
            match owner_revoke_media(&env, &body).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/stats") => api_json(&owner_stats(&env).await?, 200),
        (worker::Method::Get, "/diagnostics") => api_json(
            &serde_json::json!({ "items": owner_diagnostics(&env).await? }),
            200,
        ),
        (worker::Method::Get, "/followers") => api_json(
            &OwnerItems {
                items: owner_followers(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/friends") => api_json(
            &OwnerItems {
                items: owner_friends(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/following") => api_json(
            &OwnerItems {
                items: owner_following(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/audience-lists") => api_json(
            &OwnerItems {
                items: owner_audience_lists(&env).await?,
            },
            200,
        ),
        (worker::Method::Post, "/audience-lists") => {
            let body = read_json(&mut req).await;
            match owner_upsert_audience_list(&env, &body).await {
                Ok(list) => api_json(&list, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/audience-lists/") => {
            let id = decode_component(owner_path.trim_start_matches("/audience-lists/"));
            if id.trim().is_empty() {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            }
            owner_delete_audience_list(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Post, "/discovery/actor") => {
            let body = read_json(&mut req).await;
            let target = string_like_any(&body, &["target"]).unwrap_or_default();
            match owner_discover_actor(&env, target.trim()).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/following/follow") => {
            let body = read_json(&mut req).await;
            let target = string_like_any(&body, &["target"]).unwrap_or_default();
            match owner_follow_actor(&env, target.trim()).await {
                Ok(result) => api_json(&result, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/following/unfollow") => {
            let body = read_json(&mut req).await;
            let target = string_like_any(&body, &["target"]).unwrap_or_default();
            match owner_unfollow_actor(&env, target.trim()).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/posts") => api_json(
            &OwnerItems {
                items: owner_posts(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, "/posts") => {
            let body = read_json(&mut req).await;
            let Some(text) = body_string_any(&body, &["text", "content"]) else {
                return api_json(&serde_json::json!({ "error": "text is required" }), 400);
            };
            let Some(visibility) = normalize_visibility(
                string_like_any(&body, &["visibility"])
                    .unwrap_or_else(|| "followers".to_string())
                    .as_str(),
            ) else {
                return api_json(
                    &serde_json::json!({ "error": "unsupported visibility" }),
                    400,
                );
            };
            let Some(protocol) = normalize_protocol(
                string_like_any(&body, &["protocol"])
                    .unwrap_or_else(|| "activitypub".to_string())
                    .as_str(),
            ) else {
                return api_json(&serde_json::json!({ "error": "unsupported protocol" }), 400);
            };
            if matches!(visibility.as_str(), "followers" | "direct") && protocol == "atproto" {
                return api_json(
                    &serde_json::json!({ "error": "private posts cannot route only to atproto" }),
                    400,
                );
            }
            let recipients = body
                .get("recipients")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|value| optional_body_string(value))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let attachments = body
                .get("attachments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let encrypt = body.get("encrypt").map(js_truthy).unwrap_or(false);
            let in_reply_to =
                body_string_any(&body, &["in_reply_to", "inReplyTo", "in_reply_to_id"]);
            let audience_list_id = body_string_any(&body, &["audience_list_id", "audienceListId"]);
            if visibility == "direct"
                && recipients.is_empty()
                && audience_list_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
            {
                return api_json(
                    &serde_json::json!({ "error": "direct posts require at least one recipient" }),
                    400,
                );
            }
            match owner_create_post(
                &env,
                &text,
                &visibility,
                &protocol,
                recipients,
                attachments,
                encrypt,
                in_reply_to,
                audience_list_id,
                "Note",
                None,
                None,
            )
            .await
            {
                Ok(created) => api_json(&created, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, _) if owner_path.starts_with("/posts/") => {
            let post_id = decode_component(owner_path.trim_start_matches("/posts/"));
            match owner_post_detail(&env, &post_id).await? {
                Some(post) => api_json(&post, 200),
                None => api_json(&serde_json::json!({ "error": "post not found" }), 404),
            }
        }
        (worker::Method::Get, "/saved") => api_json(
            &OwnerItems {
                items: owner_saved_posts(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, "/saved") => {
            let body = read_json(&mut req).await;
            match owner_save_post(&env, &body).await {
                Ok(saved) => api_json(&saved, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/saved/") => {
            let id = decode_component(owner_path.trim_start_matches("/saved/"));
            if id.trim().is_empty() {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            }
            owner_unsave_post(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/posts/") => {
            let post_id = decode_component(owner_path.trim_start_matches("/posts/"));
            match owner_delete_post(&env, &post_id).await? {
                Some(deleted) => api_json(&deleted, 200),
                None => api_json(&serde_json::json!({ "error": "post not found" }), 404),
            }
        }
        (worker::Method::Get, "/timeline/home") => api_json(
            &OwnerItems {
                items: owner_home_timeline(
                    &env,
                    limit,
                    query_param(url, "include_replies").as_deref() == Some("true"),
                )
                .await?,
            },
            200,
        ),
        (worker::Method::Get, "/notifications") => api_json(
            &OwnerItems {
                items: owner_notifications(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, "/notifications/read") => {
            let body = read_json(&mut req).await;
            let Some(id) = required_body_string(body.get("id")) else {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            };
            owner_mark_notification_read(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Get, "/deliveries") => api_json(
            &OwnerItems {
                items: owner_deliveries(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, _) if owner_path.starts_with("/deliveries/") => {
            match owner_delivery_action_path(owner_path) {
                Some((delivery_id, action)) => {
                    match owner_update_delivery_status(&env, delivery_id.as_str(), action).await {
                        Ok(delivery) => api_json(&delivery, 200),
                        Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
                    }
                }
                None => api_json(
                    &serde_json::json!({ "error": "unsupported delivery action" }),
                    404,
                ),
            }
        }
        (worker::Method::Get, "/direct-messages") => api_json(
            &OwnerItems {
                items: owner_direct_messages(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/e2ee/messages") => api_json(
            &OwnerItems {
                items: owner_e2ee_messages(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Delete, _) if owner_path.starts_with("/e2ee/messages/") => {
            let message_id = decode_component(owner_path.trim_start_matches("/e2ee/messages/"));
            if message_id.trim().is_empty() {
                return api_json(
                    &serde_json::json!({ "error": "message id is required" }),
                    400,
                );
            }
            match owner_delete_e2ee_message(&env, &message_id).await? {
                true => api_json(&serde_json::json!({ "ok": true }), 200),
                false => api_json(&serde_json::json!({ "error": "message not found" }), 404),
            }
        }
        (worker::Method::Post, "/e2ee/messages") => {
            let body = read_json(&mut req).await;
            match owner_send_e2ee_message(&env, &body).await {
                Ok(message) => api_json(&message, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/e2ee/devices") => api_json(
            &OwnerItems {
                items: owner_e2ee_devices(&env).await?,
            },
            200,
        ),
        (worker::Method::Post, "/e2ee/devices") => {
            let body = read_json(&mut req).await;
            match owner_upsert_e2ee_device(&env, &body).await {
                Ok(device) => api_json(&device, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/e2ee/devices/revoke") => {
            let body = read_json(&mut req).await;
            match owner_revoke_e2ee_device(&env, &body).await {
                Ok(device) => api_json(&device, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/e2ee/peers") => api_json(
            &OwnerItems {
                items: owner_e2ee_peer_devices(&env).await?,
            },
            200,
        ),
        (worker::Method::Post, "/e2ee/peers/discover") => {
            let body = read_json(&mut req).await;
            match owner_discover_e2ee_peer_devices(&env, &body).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/e2ee/peers/trust") => {
            let body = read_json(&mut req).await;
            match owner_trust_e2ee_peer_device(&env, &body).await {
                Ok(device) => api_json(&device, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/e2ee/peers/revoke") => {
            let body = read_json(&mut req).await;
            match owner_revoke_e2ee_peer_device(&env, &body).await {
                Ok(device) => api_json(&device, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/search") => api_json(
            &owner_search(
                &env,
                query_param(url, "q").unwrap_or_default(),
                limit,
                owner_search_flags(url),
            )
            .await?,
            200,
        ),
        (worker::Method::Get, "/sources") => api_json(
            &OwnerSources {
                subscriptions: owner_source_subscriptions(&env, limit).await?,
                items: owner_source_items(
                    &env,
                    clamp_limit(query_param(url, "items_limit").or_else(|| Some("40".to_string()))),
                )
                .await?,
            },
            200,
        ),
        (worker::Method::Post, "/sources") => {
            let body = read_json(&mut req).await;
            match owner_add_source(&env, &body).await {
                Ok(source) => api_json(&serde_json::json!({ "ok": true, "source": source }), 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/sources/refresh") => {
            let body = read_json(&mut req).await;
            let id = body.get("id").and_then(optional_body_string);
            match owner_refresh_sources(&env, id.as_deref()).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/watches") => api_json(
            &OwnerSources {
                subscriptions: owner_watch_subscriptions(&env, limit).await?,
                items: owner_watch_items(
                    &env,
                    clamp_limit(query_param(url, "items_limit").or_else(|| Some("40".to_string()))),
                )
                .await?,
            },
            200,
        ),
        (worker::Method::Post, "/watches") => {
            let body = read_json(&mut req).await;
            match owner_add_watch(&env, &body).await {
                Ok(source) => api_json(&serde_json::json!({ "ok": true, "source": source }), 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/watches/refresh") => {
            let body = read_json(&mut req).await;
            let id = body.get("id").and_then(optional_body_string);
            match owner_refresh_watches(&env, id.as_deref()).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/watches/") => {
            let id = decode_component(owner_path.trim_start_matches("/watches/"));
            if id.trim().is_empty() {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            }
            match owner_delete_watch(&env, &id).await {
                Ok(()) => api_json(&serde_json::json!({ "ok": true }), 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/sources/") => {
            let id = decode_component(owner_path.trim_start_matches("/sources/"));
            if id.trim().is_empty() {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            }
            owner_delete_source(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Post, "/followers/status") => {
            let body = read_json(&mut req).await;
            let follower_actor_id = string_like_field(&body, "follower_actor_id")
                .unwrap_or_default()
                .trim()
                .to_string();
            let status = string_like_field(&body, "status")
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            match owner_set_follower_status(&env, &follower_actor_id, &status).await {
                Ok(result) => api_json(&result, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Get, "/moderation") => api_json(&owner_moderation(&env).await?, 200),
        (worker::Method::Get, "/moderation/replies") => api_json(
            &OwnerItems {
                items: owner_moderation_replies(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, "/moderation/replies/status") => {
            let body = read_json(&mut req).await;
            let Some(reply_id) = body_string_any(&body, &["reply_id", "replyId", "id"]) else {
                return api_json(&serde_json::json!({ "error": "reply_id is required" }), 400);
            };
            let Some(status) = body_string_any(&body, &["status"]) else {
                return api_json(&serde_json::json!({ "error": "status is required" }), 400);
            };
            match owner_set_reply_moderation_status(&env, &reply_id, &status).await {
                Ok(reply) => api_json(&reply, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/moderation/settings") => {
            let body = read_json(&mut req).await;
            match owner_update_moderation_settings(&env, &body).await {
                Ok(settings) => api_json(&settings, 200),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/moderation/block") => {
            let body = read_json(&mut req).await;
            match owner_block(&env, &body).await {
                Ok(block) => api_json(&block, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        (worker::Method::Post, "/moderation/unblock") => {
            let body = read_json(&mut req).await;
            let Some(value) = body_string_any(&body, &["value", "actor_id", "actorId", "domain"])
            else {
                return api_json(&serde_json::json!({ "error": "value is required" }), 400);
            };
            owner_unblock(&env, &value).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Post, "/moderation/allowlist") => {
            let body = read_json(&mut req).await;
            let host = match normalize_host_value(
                body.get("host")
                    .and_then(optional_body_string)
                    .as_deref()
                    .unwrap_or_default(),
            ) {
                Ok(host) => host,
                Err(message) => {
                    return api_json(&serde_json::json!({ "error": message }), 400);
                }
            };
            let note = body.get("note").and_then(optional_body_string);
            api_json(&owner_allow_host(&env, &host, note.as_deref()).await?, 201)
        }
        (worker::Method::Delete, _) if owner_path.starts_with("/moderation/allowlist/") => {
            let host = normalize_host(&decode_component(
                owner_path.trim_start_matches("/moderation/allowlist/"),
            ))?;
            owner_delete_allowlist_host(&env, &host).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Post, "/interactions") => {
            let body = read_json(&mut req).await;
            let object_id = body_string_any(&body, &["object_id", "objectId"]).unwrap_or_default();
            let interaction = body_string_any(&body, &["interaction", "action"])
                .unwrap_or_default()
                .to_ascii_lowercase();
            match owner_publish_interaction(&env, &object_id, &interaction).await {
                Ok(result) => api_json(&result, 201),
                Err(message) => api_json(&serde_json::json!({ "error": message }), 400),
            }
        }
        _ => api_json(
            &serde_json::json!({ "error": "Rust router migration scaffold: owner route not migrated yet" }),
            501,
        ),
    }
}

fn owner_api_required_scopes(method: worker::Method, path: &str) -> &'static [&'static str] {
    match method {
        _ if path == "/discovery/actor" => &["read"],
        _ if path == "/followers/status"
            || path == "/following/follow"
            || path == "/following/unfollow" =>
        {
            &["follow"]
        }
        _ if path.starts_with("/moderation/") => &["moderation"],
        _ if method != worker::Method::Get
            && (path == "/settings" || path.starts_with("/deliveries/")) =>
        {
            &["write"]
        }
        worker::Method::Get => &["read"],
        worker::Method::Delete => &["write"],
        _ if path == "/media" || path == "/media/revoke" => &["media"],
        _ => &["write"],
    }
}

async fn owner_profile(env: &Env) -> Result<OwnerProfile> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type,
                   display_name, summary, icon, image, avatar_url, header_url
            FROM actors
            WHERE username = 'social'
            LIMIT 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let username = string_field(row.as_ref(), "username").unwrap_or_else(|| "social".to_string());
    let actor_url = string_field(row.as_ref(), "id").unwrap_or_else(|| local_actor_url(env));
    let actor_type =
        string_field(row.as_ref(), "actor_type").unwrap_or_else(|| "Person".to_string());
    let handle_domain = env
        .var("DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());
    let icon = string_field(row.as_ref(), "icon");
    let image = string_field(row.as_ref(), "image");
    Ok(OwnerProfile {
        id: actor_url.clone(),
        username: username.clone(),
        actor_type,
        display_name: string_field(row.as_ref(), "display_name"),
        summary: string_field(row.as_ref(), "summary"),
        avatar_url: string_field(row.as_ref(), "avatar_url").or_else(|| icon.clone()),
        header_url: string_field(row.as_ref(), "header_url").or_else(|| image.clone()),
        icon,
        image,
        public_handle: format!("@{username}@{handle_domain}"),
        actor_url,
    })
}

async fn owner_update_profile(
    env: &Env,
    body: &Value,
) -> std::result::Result<OwnerProfile, String> {
    let actor_type = body.get("actor_type").and_then(optional_body_string);
    if let Some(actor_type) = actor_type.as_deref() {
        if !matches!(actor_type, "Person" | "Group" | "Organization") {
            return Err("actor_type must be Person, Group, or Organization".to_string());
        }
    }
    let display_name = body.get("display_name").and_then(optional_body_string);
    let summary = body.get("summary").and_then(optional_body_string);
    let icon = optional_url_field(body, "icon", "icon")?;
    let image = optional_url_field(body, "image", "image")?;

    if actor_type.is_none()
        && display_name.is_none()
        && summary.is_none()
        && icon.is_none()
        && image.is_none()
    {
        return Err("no profile fields provided".to_string());
    }

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let actor_type_arg = actor_type
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let display_name_arg = display_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let summary_arg = summary.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let icon_arg = icon.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let image_arg = image.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let actor_type_present = D1Type::Integer(if actor_type.is_some() { 1 } else { 0 });
    let display_name_present = D1Type::Integer(if display_name.is_some() { 1 } else { 0 });
    let summary_present = D1Type::Integer(if summary.is_some() { 1 } else { 0 });
    let icon_present = D1Type::Integer(if icon.is_some() { 1 } else { 0 });
    let image_present = D1Type::Integer(if image.is_some() { 1 } else { 0 });
    db.prepare(
        r#"
        UPDATE actors
        SET updated_at = CURRENT_TIMESTAMP,
            actor_type = CASE WHEN ?1 = 1 THEN ?2 ELSE actor_type END,
            display_name = CASE WHEN ?3 = 1 THEN ?4 ELSE display_name END,
            summary = CASE WHEN ?5 = 1 THEN ?6 ELSE summary END,
            icon = CASE WHEN ?7 = 1 THEN ?8 ELSE icon END,
            avatar_url = CASE WHEN ?7 = 1 THEN ?8 ELSE avatar_url END,
            image = CASE WHEN ?9 = 1 THEN ?10 ELSE image END,
            header_url = CASE WHEN ?9 = 1 THEN ?10 ELSE header_url END
        WHERE username = 'social'
        "#,
    )
    .bind_refs([
        &actor_type_present,
        &actor_type_arg,
        &display_name_present,
        &display_name_arg,
        &summary_present,
        &summary_arg,
        &icon_present,
        &icon_arg,
        &image_present,
        &image_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    owner_profile(env).await.map_err(|error| error.to_string())
}

async fn owner_upload_media(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let filename = body
        .get("filename")
        .and_then(optional_body_string)
        .ok_or_else(|| "filename is required".to_string())?;
    let data_base64 = body
        .get("data_base64")
        .and_then(optional_body_string)
        .ok_or_else(|| "data_base64 is required".to_string())?;
    let media_type = body
        .get("media_type")
        .and_then(optional_body_string)
        .unwrap_or_else(|| media_type_for_filename(&filename));
    let access = body
        .get("access")
        .and_then(optional_body_string)
        .unwrap_or_else(|| "public".to_string());
    let require_authorized_fetch = body
        .get("require_authorized_fetch")
        .or_else(|| body.get("requireAuthorizedFetch"))
        .map(js_truthy)
        .unwrap_or(false);
    let expires_at = private_media_expires_at(
        body.get("expires_in_seconds")
            .or_else(|| body.get("expiresInSeconds")),
    )?;

    if !allowed_media_type(&media_type) {
        return Err("unsupported media type".to_string());
    }
    if !matches!(access.as_str(), "public" | "private") {
        return Err("access must be public or private".to_string());
    }
    if expires_at.is_some() && access != "private" {
        return Err("media expiration is only supported for private uploads".to_string());
    }
    if require_authorized_fetch && access != "private" {
        return Err("authorized-fetch media is only supported for private uploads".to_string());
    }

    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|error| error.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("media file is larger than 8 MB".to_string());
    }

    let safe_name = safe_media_filename(&filename)?;
    let timestamp = current_media_timestamp();
    let created_at = current_media_created_at();
    let token = random_token()?;
    let public_name = format!(
        "{}-{}-{}",
        timestamp,
        stable_id(&format!("{safe_name}\n{data_base64}"))
            .chars()
            .take(12)
            .collect::<String>(),
        safe_name
    );
    let key = if access == "private" {
        format!("private/{token}/{safe_name}")
    } else {
        format!("uploads/{public_name}")
    };

    let description = body.get("description").and_then(optional_body_string);
    let actor_url = local_actor_url(env);
    let custom_metadata = media_custom_metadata(MediaMetadataInput {
        owner: &actor_url,
        access: &access,
        media_type: &media_type,
        bytes: &bytes,
        created_at: &created_at,
        description: description.as_deref(),
        expires_at: expires_at.as_deref(),
        require_authorized_fetch,
    });
    let media_size = bytes.len() as u64;
    let media_hash = custom_metadata
        .get("sha256")
        .cloned()
        .unwrap_or_else(String::new);

    let mut http_metadata = worker::HttpMetadata::default();
    http_metadata.content_type = Some(media_type.clone());
    let bucket = env
        .bucket("MEDIA_BUCKET")
        .map_err(|error| error.to_string())?;
    let put = bucket.put(key.clone(), bytes).http_metadata(http_metadata);
    if custom_metadata.is_empty() {
        put.execute().await.map_err(|error| error.to_string())?;
    } else {
        put.custom_metadata(custom_metadata)
            .execute()
            .await
            .map_err(|error| error.to_string())?;
    }

    let url = if access == "private" {
        format!(
            "https://{}/media/{}/{}/{}",
            activitypub_domain(env),
            if require_authorized_fetch {
                "_private_signed"
            } else {
                "_private"
            },
            token,
            safe_name
        )
    } else {
        format!("https://{}/media/{key}", activitypub_domain(env))
    };
    let mut attachment = Map::new();
    attachment.insert(
        "type".to_string(),
        Value::String(if media_type.starts_with("image/") {
            "Image".to_string()
        } else {
            "Document".to_string()
        }),
    );
    attachment.insert("mediaType".to_string(), Value::String(media_type.clone()));
    attachment.insert("url".to_string(), Value::String(url.clone()));
    attachment.insert(
        "name".to_string(),
        Value::String(description.clone().unwrap_or(safe_name)),
    );

    let mut response = Map::new();
    response.insert("url".to_string(), Value::String(url));
    response.insert("media_type".to_string(), Value::String(media_type));
    response.insert("access".to_string(), Value::String(access));
    response.insert("owner".to_string(), Value::String(actor_url));
    response.insert("size".to_string(), Value::from(media_size));
    response.insert("hash".to_string(), Value::String(media_hash));
    response.insert("created_at".to_string(), Value::String(created_at));
    response.insert(
        "authorized_fetch".to_string(),
        Value::Bool(require_authorized_fetch),
    );
    response.insert("attachment".to_string(), Value::Object(attachment));
    response.insert(
        "description".to_string(),
        description.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "expires_at".to_string(),
        expires_at.map(Value::String).unwrap_or(Value::Null),
    );
    Ok(response)
}

async fn owner_revoke_media(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let url = body_string_any(body, &["url", "media_url", "id"]).unwrap_or_default();
    let Some(key) = media_r2_key_from_url(&url) else {
        return Err("valid media url is required".to_string());
    };
    env.bucket("MEDIA_BUCKET")
        .map_err(|error| error.to_string())?
        .delete(key.clone())
        .await
        .map_err(|error| error.to_string())?;

    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("url".to_string(), Value::String(url));
    response.insert("key".to_string(), Value::String(key));
    Ok(response)
}

async fn owner_stats(env: &Env) -> Result<OwnerStats> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT
                (SELECT COUNT(*) FROM followers) AS followers_total,
                (SELECT COUNT(*) FROM followers WHERE status='approved') AS followers_approved,
                (SELECT COUNT(*) FROM followers WHERE status='pending') AS followers_pending,
                (SELECT COUNT(*) FROM followers WHERE status='rejected') AS followers_rejected,
                (SELECT COUNT(*) FROM following) AS following_total,
                (SELECT COUNT(*) FROM posts) AS posts_total,
                (SELECT COUNT(*) FROM activities) AS activities_total,
                (SELECT COUNT(*) FROM deliveries) AS deliveries_total,
                (SELECT COUNT(*) FROM deliveries WHERE status='failed') AS deliveries_failed,
                (SELECT COUNT(*) FROM deliveries WHERE status='queued') AS deliveries_queued,
                (SELECT COUNT(*) FROM deliveries WHERE status='retry') AS deliveries_retry,
                (SELECT COUNT(*) FROM deliveries WHERE status='delivered') AS deliveries_delivered,
                (SELECT COUNT(*) FROM posts WHERE protocol='both') AS dual_protocol_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility='public') AS public_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility IN ('followers', 'unlisted')) AS private_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility='direct') AS direct_posts,
                (SELECT COUNT(*) FROM posts WHERE encrypted_message IS NOT NULL) AS encrypted_posts,
                (SELECT COUNT(*) FROM posts WHERE media_attachments IS NOT NULL AND media_attachments != '') AS media_posts,
                (SELECT COUNT(*) FROM notifications WHERE read = 0 OR read IS NULL) AS notifications_unread,
                (SELECT COUNT(*) FROM blocks) AS blocks_total,
                (SELECT COUNT(*) FROM federation_allowlist WHERE enabled = 1) AS allowlist_hosts,
                (SELECT closed_network FROM instance_settings WHERE id = 1) AS closed_network
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerStats {
        followers_total: integer_field(row.as_ref(), "followers_total"),
        followers_approved: integer_field(row.as_ref(), "followers_approved"),
        followers_pending: integer_field(row.as_ref(), "followers_pending"),
        followers_rejected: integer_field(row.as_ref(), "followers_rejected"),
        following_total: integer_field(row.as_ref(), "following_total"),
        posts_total: integer_field(row.as_ref(), "posts_total"),
        activities_total: integer_field(row.as_ref(), "activities_total"),
        deliveries_total: integer_field(row.as_ref(), "deliveries_total"),
        deliveries_failed: integer_field(row.as_ref(), "deliveries_failed"),
        deliveries_queued: integer_field(row.as_ref(), "deliveries_queued"),
        deliveries_retry: integer_field(row.as_ref(), "deliveries_retry"),
        deliveries_delivered: integer_field(row.as_ref(), "deliveries_delivered"),
        dual_protocol_posts: integer_field(row.as_ref(), "dual_protocol_posts"),
        public_posts: integer_field(row.as_ref(), "public_posts"),
        private_posts: integer_field(row.as_ref(), "private_posts"),
        direct_posts: integer_field(row.as_ref(), "direct_posts"),
        encrypted_posts: integer_field(row.as_ref(), "encrypted_posts"),
        media_posts: integer_field(row.as_ref(), "media_posts"),
        notifications_unread: integer_field(row.as_ref(), "notifications_unread"),
        blocks_total: integer_field(row.as_ref(), "blocks_total"),
        allowlist_hosts: integer_field(row.as_ref(), "allowlist_hosts"),
        closed_network: integer_field(row.as_ref(), "closed_network") != 0,
    })
}

async fn owner_diagnostics(env: &Env) -> Result<Vec<OwnerDiagnostic>> {
    let db = env.d1("DB")?;
    let settings = db
        .prepare(
            r#"
            SELECT default_visibility
            FROM instance_settings
            WHERE id = 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let posts = db
        .prepare("SELECT COUNT(*) AS count FROM posts")
        .first::<Map<String, Value>>(None)
        .await?;
    let followers = db
        .prepare("SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'")
        .first::<Map<String, Value>>(None)
        .await?;
    let deliveries = db
        .prepare("SELECT status, COUNT(*) AS count FROM deliveries GROUP BY status")
        .all()
        .await?
        .results::<DeliveryCount>()?;
    let default_visibility = string_field(settings.as_ref(), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    let failed_deliveries = deliveries
        .iter()
        .find(|row| row.status == "failed")
        .map(|row| row.count)
        .unwrap_or(0);
    let delivery_detail = if deliveries.is_empty() {
        "no deliveries".to_string()
    } else {
        deliveries
            .iter()
            .map(|row| format!("{}={}", row.status, row.count))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Ok(vec![
        OwnerDiagnostic {
            key: "owner-api",
            ok: true,
            detail: "Authenticated owner API is available.".to_string(),
        },
        OwnerDiagnostic {
            key: "private-default",
            ok: default_visibility == "followers",
            detail: format!("default visibility is {default_visibility}"),
        },
        OwnerDiagnostic {
            key: "activitypub",
            ok: true,
            detail: format!(
                "posts={} approved_followers={}",
                integer_field(posts.as_ref(), "count"),
                integer_field(followers.as_ref(), "count")
            ),
        },
        OwnerDiagnostic {
            key: "deliveries",
            ok: failed_deliveries == 0,
            detail: delivery_detail,
        },
    ])
}

async fn owner_settings(env: &Env) -> Result<Map<String, Value>> {
    let db = env.d1("DB")?;
    let has_default_protocol = column_exists(env, "instance_settings", "default_protocol").await?;
    let select = if has_default_protocol {
        r#"
        SELECT default_visibility,
               COALESCE(default_protocol, 'activitypub') AS default_protocol,
               require_authorized_fetch, manually_approves_followers,
               COALESCE(closed_network, 0) AS closed_network
        FROM instance_settings
        WHERE id = 1
        "#
    } else {
        r#"
        SELECT default_visibility, require_authorized_fetch, manually_approves_followers,
               COALESCE(closed_network, 0) AS closed_network
        FROM instance_settings
        WHERE id = 1
        "#
    };
    Ok(db
        .prepare(select)
        .first::<Map<String, Value>>(None)
        .await?
        .map(|mut settings| {
            settings
                .entry("default_protocol".to_string())
                .or_insert_with(|| Value::String("activitypub".to_string()));
            settings
        })
        .unwrap_or_else(|| {
            let mut settings = Map::new();
            settings.insert(
                "default_visibility".to_string(),
                Value::String("followers".to_string()),
            );
            settings.insert(
                "default_protocol".to_string(),
                Value::String("activitypub".to_string()),
            );
            settings.insert("require_authorized_fetch".to_string(), Value::from(1));
            settings.insert("manually_approves_followers".to_string(), Value::from(1));
            settings.insert("closed_network".to_string(), Value::from(0));
            settings
        }))
}

async fn owner_snapshot_settings(env: &Env) -> Result<Map<String, Value>> {
    let settings = owner_settings(env).await?;
    let default_visibility = string_field(Some(&settings), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    let default_protocol = string_field(Some(&settings), "default_protocol")
        .unwrap_or_else(|| "activitypub".to_string());
    let mut snapshot_settings = Map::new();
    snapshot_settings.insert(
        "instance_url".to_string(),
        Value::String(owner_instance_url(env)),
    );
    snapshot_settings.insert("owner_token_present".to_string(), Value::Bool(true));
    snapshot_settings.insert(
        "default_visibility".to_string(),
        Value::String(title_visibility(Some(default_visibility.as_str()))),
    );
    snapshot_settings.insert(
        "default_protocol".to_string(),
        Value::String(title_protocol(Some(default_protocol.as_str()))),
    );
    Ok(snapshot_settings)
}

async fn owner_update_settings(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let Some(default_visibility) = normalize_visibility(
        string_like_any(body, &["default_visibility", "defaultVisibility"])
            .unwrap_or_else(|| "followers".to_string())
            .as_str(),
    ) else {
        return Err("unsupported default_visibility".to_string());
    };
    let Some(default_protocol) = normalize_protocol(
        string_like_any(body, &["default_protocol", "defaultProtocol"])
            .unwrap_or_else(|| "activitypub".to_string())
            .as_str(),
    ) else {
        return Err("unsupported default_protocol".to_string());
    };
    if matches!(default_visibility.as_str(), "followers" | "direct")
        && default_protocol == "atproto"
    {
        return Err("private defaults cannot route only to atproto".to_string());
    }
    let require_authorized_fetch = body
        .get("require_authorized_fetch")
        .or_else(|| body.get("requireAuthorizedFetch"))
        .map(js_truthy)
        .unwrap_or(true);
    let manually_approves_followers = body
        .get("manually_approves_followers")
        .or_else(|| body.get("manuallyApprovesFollowers"))
        .map(js_truthy)
        .unwrap_or(true);
    let closed_network = body
        .get("closed_network")
        .or_else(|| body.get("closedNetwork"))
        .map(js_truthy)
        .unwrap_or(false);

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let default_visibility_arg = D1Type::Text(&default_visibility);
    let default_protocol_arg = D1Type::Text(&default_protocol);
    let require_arg = D1Type::Integer(if require_authorized_fetch { 1 } else { 0 });
    let manual_arg = D1Type::Integer(if manually_approves_followers { 1 } else { 0 });
    let closed_arg = D1Type::Integer(if closed_network { 1 } else { 0 });
    db.prepare(
        r#"
        INSERT INTO instance_settings (
            id, default_visibility, default_protocol, require_authorized_fetch,
            manually_approves_followers, closed_network, updated_at
        ) VALUES (
            1, ?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP
        )
        ON CONFLICT(id) DO UPDATE SET
            default_visibility = excluded.default_visibility,
            default_protocol = excluded.default_protocol,
            require_authorized_fetch = excluded.require_authorized_fetch,
            manually_approves_followers = excluded.manually_approves_followers,
            closed_network = excluded.closed_network,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs(&[
        default_visibility_arg,
        default_protocol_arg,
        require_arg,
        manual_arg,
        closed_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_snapshot_settings(env)
        .await
        .map_err(|error| error.to_string())
}

async fn owner_snapshot(env: &Env) -> Result<Map<String, Value>> {
    let profile = owner_profile(env).await?;
    let home_timeline = owner_home_timeline(env, 20, false).await?;
    let posts = owner_posts(env, 20).await?;
    let saved_posts = owner_saved_posts(env, 20).await?;
    let followers = owner_followers(env, 100).await?;
    let friends = owner_friends(env, 100).await?;
    let following = owner_following(env, 100).await?;
    let audience_lists = owner_audience_lists(env).await?;
    let sources = owner_source_items(env, 20).await?;
    let moderation = owner_moderation(env).await?;
    let diagnostics = owner_diagnostics(env).await?;
    let snapshot_settings = owner_snapshot_settings(env).await?;

    let mut snapshot = Map::new();
    snapshot.insert("settings".to_string(), Value::Object(snapshot_settings));
    snapshot.insert(
        "active_section".to_string(),
        Value::String("Home".to_string()),
    );
    snapshot.insert("profile".to_string(), serde_json::json!(profile));
    snapshot.insert(
        "home_timeline".to_string(),
        Value::Array(
            home_timeline
                .into_iter()
                .map(shape_snapshot_home_timeline_item)
                .map(Value::Object)
                .collect(),
        ),
    );
    snapshot.insert(
        "posts".to_string(),
        Value::Array(
            posts
                .into_iter()
                .map(shape_snapshot_post)
                .map(Value::Object)
                .collect(),
        ),
    );
    snapshot.insert(
        "saved_posts".to_string(),
        Value::Array(saved_posts.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "followers".to_string(),
        Value::Array(followers.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "friends".to_string(),
        Value::Array(friends.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "following".to_string(),
        Value::Array(following.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "audience_lists".to_string(),
        Value::Array(audience_lists.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "sources".to_string(),
        Value::Array(sources.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert("moderation".to_string(), serde_json::json!(moderation));
    snapshot.insert("diagnostics".to_string(), serde_json::json!(diagnostics));
    Ok(snapshot)
}

fn shape_snapshot_home_timeline_item(post: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&post, "id"));
    item.insert(
        "object_id".to_string(),
        row_value_or_null(&post, "object_id"),
    );
    item.insert("actor_id".to_string(), row_value_or_null(&post, "actor_id"));
    item.insert(
        "actor_username".to_string(),
        row_value_or_null(&post, "actor_username"),
    );
    item.insert(
        "actor_display_name".to_string(),
        row_value_or_null(&post, "actor_display_name"),
    );
    item.insert(
        "actor_avatar_url".to_string(),
        row_value_or_null(&post, "actor_avatar_url"),
    );
    item.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    item.insert(
        "content_html".to_string(),
        row_value_or_null(&post, "content_html"),
    );
    item.insert(
        "visibility".to_string(),
        Value::String(
            string_field(Some(&post), "visibility").unwrap_or_else(|| "public".to_string()),
        ),
    );
    item.insert(
        "in_reply_to".to_string(),
        row_value_or_null(&post, "in_reply_to"),
    );
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    item.insert(
        "protocol".to_string(),
        Value::String(
            string_field(Some(&post), "protocol").unwrap_or_else(|| "activitypub".to_string()),
        ),
    );
    item.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    item.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    item.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    item
}

fn shape_snapshot_post(post: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&post, "id"));
    item.insert("title".to_string(), row_value_or_null(&post, "name"));
    item.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    item.insert(
        "visibility".to_string(),
        Value::String(title_visibility(
            string_field(Some(&post), "visibility").as_deref(),
        )),
    );
    item.insert(
        "protocol".to_string(),
        Value::String(title_protocol(
            string_field(Some(&post), "protocol").as_deref(),
        )),
    );
    item.insert(
        "encrypted".to_string(),
        Value::Bool(non_empty_value(&post, "encrypted_message").is_some()),
    );
    item.insert(
        "attachments".to_string(),
        Value::Array(parse_attachment_array(post.get("media_attachments"))),
    );
    item.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    item.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    item.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    item
}

async fn owner_moderation(env: &Env) -> Result<OwnerModeration> {
    let db = env.d1("DB")?;
    owner_refresh_reply_moderation(env, 120).await?;
    let settings = owner_settings(env).await?;
    let moderation_settings = owner_moderation_settings(env).await?;
    let blocks = db
        .prepare("SELECT COUNT(*) AS count FROM blocks")
        .first::<Map<String, Value>>(None)
        .await?;
    let allowlist = db
        .prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1")
        .first::<Map<String, Value>>(None)
        .await?;
    let reply_counts = db
        .prepare(
            r#"
            SELECT
                COUNT(*) AS total_count,
                SUM(CASE WHEN moderation_status = 'pending' THEN 1 ELSE 0 END) AS pending_count,
                SUM(CASE WHEN moderation_status = 'hidden' THEN 1 ELSE 0 END) AS hidden_count,
                SUM(CASE WHEN moderation_status = 'rejected' THEN 1 ELSE 0 END) AS rejected_count,
                SUM(
                    CASE
                        WHEN moderation_flags IS NOT NULL
                         AND moderation_flags != ''
                         AND moderation_flags != '[]'
                        THEN 1
                        ELSE 0
                    END
                ) AS flagged_count
            FROM replies
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerModeration {
        closed_network: bool_field(Some(&settings), "closed_network"),
        block_count: integer_field(blocks.as_ref(), "count"),
        allowlist_count: integer_field(allowlist.as_ref(), "count"),
        require_authorized_fetch: bool_field(Some(&settings), "require_authorized_fetch"),
        manually_approves_followers: bool_field(Some(&settings), "manually_approves_followers"),
        reply_policy: string_field(moderation_settings.as_ref(), "reply_policy")
            .unwrap_or_else(|| "warn".to_string()),
        ai_enabled: bool_field(moderation_settings.as_ref(), "ai_enabled"),
        ai_model: string_field(moderation_settings.as_ref(), "ai_model"),
        ai_daily_budget: integer_field(moderation_settings.as_ref(), "ai_daily_budget"),
        reply_queue_count: integer_field(reply_counts.as_ref(), "pending_count"),
        flagged_reply_count: integer_field(reply_counts.as_ref(), "flagged_count"),
        hidden_reply_count: integer_field(reply_counts.as_ref(), "hidden_count"),
        rejected_reply_count: integer_field(reply_counts.as_ref(), "rejected_count"),
        blocks: owner_blocks(env).await?,
        allowlist: owner_allowlist(env).await?,
    })
}

async fn owner_moderation_settings(env: &Env) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, reply_policy, ai_enabled, ai_model, ai_daily_budget
        FROM moderation_settings
        WHERE id = 1
        LIMIT 1
        "#,
    )
    .first::<Map<String, Value>>(None)
    .await
}

async fn owner_moderation_replies(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    owner_refresh_reply_moderation(env, limit).await?;
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, post_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
                   content, published_at, created_at, moderation_status, moderation_score,
                   moderation_flags, moderation_checked_at, ai_moderation_result, hidden
            FROM replies
            WHERE moderation_status != 'approved'
               OR (hidden IS NOT NULL AND hidden != 0)
               OR (moderation_flags IS NOT NULL AND moderation_flags != '' AND moderation_flags != '[]')
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(shape_owner_moderation_reply).collect())
}

async fn owner_set_reply_moderation_status(
    env: &Env,
    reply_id: &str,
    status: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let normalized = normalize_reply_moderation_status(status)?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let reply_arg = D1Type::Text(reply_id);
    let status_arg = D1Type::Text(&normalized);
    let hidden_arg = D1Type::Integer(if normalized == "approved" { 0 } else { 1 });
    db.prepare(
        r#"
        UPDATE replies
        SET moderation_status = ?2,
            hidden = ?3,
            moderation_checked_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
    )
    .bind_refs([&reply_arg, &status_arg, &hidden_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_moderation_reply(env, reply_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "reply not found".to_string())
}

async fn owner_update_moderation_settings(
    env: &Env,
    body: &Value,
) -> std::result::Result<OwnerModeration, String> {
    let reply_policy = normalize_reply_policy(
        body_string_any(body, &["reply_policy", "replyPolicy"])
            .unwrap_or_else(|| "warn".to_string())
            .as_str(),
    )?
    .to_string();
    let ai_enabled = body
        .get("ai_enabled")
        .or_else(|| body.get("aiEnabled"))
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| optional_body_string(value).map(|v| v == "true" || v == "1"))
        })
        .unwrap_or(false);
    let ai_model = body
        .get("ai_model")
        .or_else(|| body.get("aiModel"))
        .and_then(optional_body_string);
    let ai_daily_budget = body
        .get("ai_daily_budget")
        .or_else(|| body.get("aiDailyBudget"))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(0);
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let policy_arg = D1Type::Text(&reply_policy);
    let ai_enabled_arg = D1Type::Integer(if ai_enabled { 1 } else { 0 });
    let ai_model_arg = ai_model
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let ai_daily_budget_i32 = i32::try_from(ai_daily_budget).unwrap_or(i32::MAX);
    let ai_budget_arg = D1Type::Integer(ai_daily_budget_i32);
    db.prepare(
        r#"
        INSERT INTO moderation_settings (
            id, reply_policy, ai_enabled, ai_model, ai_daily_budget, updated_at
        ) VALUES (1, ?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            reply_policy = excluded.reply_policy,
            ai_enabled = excluded.ai_enabled,
            ai_model = excluded.ai_model,
            ai_daily_budget = excluded.ai_daily_budget,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([&policy_arg, &ai_enabled_arg, &ai_model_arg, &ai_budget_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_reclassify_recent_replies(env, 120)
        .await
        .map_err(|error| error.to_string())?;
    owner_moderation(env)
        .await
        .map_err(|error| error.to_string())
}

async fn owner_reclassify_recent_replies(env: &Env, limit: i32) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, content
            FROM replies
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    for row in rows {
        let Some(reply_id) = string_field(Some(&row), "id") else {
            continue;
        };
        let content = string_field(Some(&row), "content").unwrap_or_default();
        classify_reply_in_db(env, &reply_id, &content).await?;
    }
    Ok(())
}

async fn owner_refresh_reply_moderation(env: &Env, limit: i32) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, content
            FROM replies
            WHERE moderation_checked_at IS NULL
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    for row in rows {
        let Some(reply_id) = string_field(Some(&row), "id") else {
            continue;
        };
        let content = string_field(Some(&row), "content").unwrap_or_default();
        classify_reply_in_db(env, &reply_id, &content).await?;
    }
    Ok(())
}

async fn classify_reply_in_db(
    env: &Env,
    reply_id: &str,
    content: &str,
) -> std::result::Result<(), String> {
    let settings = owner_moderation_settings(env)
        .await
        .map_err(|error| error.to_string())?;
    let policy = settings
        .as_ref()
        .and_then(|row| string_field(Some(row), "reply_policy"))
        .unwrap_or_else(|| "warn".to_string());
    let mut result = classify_reply_content(content, &policy)?;
    let ai_advisory = classify_reply_with_ai(env, settings.as_ref(), content).await?;
    if let Some(advisory) = ai_advisory.as_ref().filter(|value| value.unsafe_detected) {
        for category in &advisory.categories {
            let ai_flag = format!("ai:{category}");
            if !result.flags.contains(&ai_flag) {
                result.flags.push(ai_flag);
            }
        }
        if result.score < 0.7 {
            result.score = 0.7;
        }
    }
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let reply_arg = D1Type::Text(reply_id);
    let status_arg = D1Type::Text(&result.status);
    let score_arg = D1Type::Real(result.score);
    let flags_json = serde_json::to_string(&result.flags).map_err(|error| error.to_string())?;
    let flags_arg = D1Type::Text(&flags_json);
    let hidden_arg = D1Type::Integer(if result.hidden { 1 } else { 0 });
    let ai_result_json = ai_advisory
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    let ai_result_arg = ai_result_json
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let ai_checked_at = if ai_advisory.is_some() {
        js_sys::Date::new_0().to_iso_string().as_string()
    } else {
        None
    };
    let ai_checked_arg = ai_checked_at
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        UPDATE replies
        SET moderation_status = ?2,
            moderation_score = ?3,
            moderation_flags = ?4,
            moderation_checked_at = CURRENT_TIMESTAMP,
            hidden = ?5,
            ai_moderation_result = ?6,
            ai_moderation_checked_at = ?7
        WHERE id = ?1
        "#,
    )
    .bind_refs([
        &reply_arg,
        &status_arg,
        &score_arg,
        &flags_arg,
        &hidden_arg,
        &ai_result_arg,
        &ai_checked_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn classify_reply_with_ai(
    env: &Env,
    settings: Option<&Map<String, Value>>,
    content: &str,
) -> std::result::Result<Option<WorkersAiModerationAdvisory>, String> {
    if !bool_field(settings, "ai_enabled") {
        return Ok(None);
    }
    let daily_budget = integer_field(settings, "ai_daily_budget");
    if daily_budget <= 0 || !ai_reply_budget_available(env, daily_budget).await? {
        return Ok(None);
    }
    let model = string_field(settings, "ai_model")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "@cf/meta/llama-guard-3-8b".to_string());
    let instruction = "Classify this social reply for advisory moderation. Return JSON only with keys unsafe (boolean), categories (array of strings), and summary (string). Categories must only use: spam, harassment, medical, adult, political, family-only, work-sensitive. Mark unsafe true only when one or more categories apply.";
    let ai = env.ai("AI").map_err(|error| error.to_string())?;
    let response: Value = ai
        .run(
            model.as_str(),
            serde_json::json!({
                "messages": [
                    { "role": "system", "content": instruction },
                    { "role": "user", "content": content }
                ],
                "max_tokens": 256,
                "temperature": 0
            }),
        )
        .await
        .map_err(|error| error.to_string())?;
    let text = workers_ai_text(&response);
    let mut advisory = parse_workers_ai_moderation(&text).unwrap_or_else(|| {
        let mut categories = Vec::new();
        let lower = text.to_ascii_lowercase();
        for category in [
            "spam",
            "harassment",
            "medical",
            "adult",
            "political",
            "family-only",
            "work-sensitive",
        ] {
            if lower.contains(category) {
                categories.push(category.to_string());
            }
        }
        WorkersAiModerationAdvisory {
            model: None,
            unsafe_detected: lower.contains("unsafe") || !categories.is_empty(),
            categories,
            summary: (!text.trim().is_empty()).then(|| truncate_text(text.trim(), 240)),
        }
    });
    advisory.model = Some(model);
    advisory.categories = normalize_ai_categories(advisory.categories);
    if advisory.summary.is_none() && !text.trim().is_empty() {
        advisory.summary = Some(truncate_text(text.trim(), 240));
    }
    Ok(Some(advisory))
}

fn classify_reply_content(
    content: &str,
    policy: &str,
) -> std::result::Result<ReplyModerationDecision, String> {
    let normalized_policy = normalize_reply_policy(policy)?;
    let lower = content.to_ascii_lowercase();
    let mut flags = Vec::new();
    let mut score: f64 = 0.0;
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("buy now")
        || lower.contains("crypto")
        || lower.contains("telegram")
        || lower.contains("whatsapp")
    {
        flags.push("spam".to_string());
        score = 0.95;
    }
    if lower.contains("kill yourself")
        || lower.contains("go die")
        || lower.contains("idiot")
        || lower.contains("stupid")
        || lower.contains("moron")
    {
        if !flags.contains(&"harassment".to_string()) {
            flags.push("harassment".to_string());
        }
        score = score.max(0.85);
    }
    for category in detect_sensitive_categories(content) {
        if !flags.contains(&category) {
            flags.push(category);
        }
        score = score.max(0.55);
    }
    let (status, hidden) = if flags.is_empty() || normalized_policy == "off" {
        ("approved".to_string(), false)
    } else {
        match normalized_policy {
            "warn" => ("approved".to_string(), false),
            "review" => ("pending".to_string(), true),
            "hide" => ("hidden".to_string(), true),
            "reject" => ("rejected".to_string(), true),
            _ => ("approved".to_string(), false),
        }
    };
    Ok(ReplyModerationDecision {
        status,
        score,
        flags,
        hidden,
    })
}

fn detect_sensitive_categories(content: &str) -> Vec<String> {
    let lower = content.to_ascii_lowercase();
    let mut categories = Vec::new();
    for (label, keywords) in [
        (
            "medical",
            &[
                "medical",
                "doctor",
                "clinic",
                "hospital",
                "therapy",
                "medication",
                "prescription",
                "surgery",
                "diagnosis",
                "health",
            ][..],
        ),
        (
            "adult",
            &[
                "adult", "nsfw", "sexual", "sex", "porn", "erotic", "explicit",
            ][..],
        ),
        (
            "political",
            &[
                "political",
                "politics",
                "election",
                "vote",
                "campaign",
                "senate",
                "congress",
                "democrat",
                "republican",
            ][..],
        ),
        (
            "family-only",
            &[
                "family", "kids", "child", "children", "baby", "spouse", "partner", "wedding",
            ][..],
        ),
        (
            "work-sensitive",
            &[
                "work",
                "company",
                "employer",
                "client",
                "salary",
                "interview",
                "manager",
                "confidential",
                "internal",
                "project",
            ][..],
        ),
    ] {
        if keywords.iter().any(|keyword| lower.contains(keyword)) {
            categories.push(label.to_string());
        }
    }
    categories
}

async fn ai_reply_budget_available(
    env: &Env,
    daily_budget: i64,
) -> std::result::Result<bool, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let row = db
        .prepare(
            r#"
            SELECT COUNT(*) AS count
            FROM replies
            WHERE ai_moderation_checked_at IS NOT NULL
              AND DATE(ai_moderation_checked_at) = DATE('now')
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    Ok(integer_field(row.as_ref(), "count") < daily_budget)
}

fn workers_ai_text(value: &Value) -> String {
    value
        .get("response")
        .and_then(Value::as_str)
        .or_else(|| value.get("result").and_then(Value::as_str))
        .or_else(|| {
            value
                .get("result")
                .and_then(Value::as_object)
                .and_then(|object| object.get("response"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn parse_workers_ai_moderation(text: &str) -> Option<WorkersAiModerationAdvisory> {
    let candidate = strip_json_fence(text.trim());
    let json: Value = serde_json::from_str(candidate).ok()?;
    let unsafe_detected = json
        .get("unsafe")
        .and_then(Value::as_bool)
        .or_else(|| {
            json.get("verdict")
                .and_then(Value::as_str)
                .map(|value| value.eq_ignore_ascii_case("unsafe"))
        })
        .or_else(|| {
            json.get("safe")
                .and_then(Value::as_bool)
                .map(|value| !value)
        })
        .unwrap_or(false);
    let categories = json
        .get("categories")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let summary = json
        .get("summary")
        .or_else(|| json.get("reason"))
        .and_then(Value::as_str)
        .map(|value| truncate_text(value.trim(), 240));
    Some(WorkersAiModerationAdvisory {
        model: None,
        unsafe_detected,
        categories,
        summary,
    })
}

fn strip_json_fence(text: &str) -> &str {
    let stripped = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .unwrap_or(text)
        .trim();
    stripped.strip_suffix("```").unwrap_or(stripped).trim()
}

fn normalize_ai_categories(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim().to_ascii_lowercase();
        let canonical = match trimmed.as_str() {
            "spam" | "harassment" | "medical" | "adult" | "political" | "family-only"
            | "work-sensitive" => Some(trimmed),
            "sexual" | "nsfw" | "explicit" => Some("adult".to_string()),
            "health" => Some("medical".to_string()),
            "work" => Some("work-sensitive".to_string()),
            "family" => Some("family-only".to_string()),
            _ => None,
        };
        if let Some(category) = canonical {
            if !normalized.contains(&category) {
                normalized.push(category);
            }
        }
    }
    normalized
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let shortened: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn normalize_reply_policy(value: &str) -> std::result::Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Ok("off"),
        "warn" => Ok("warn"),
        "review" => Ok("review"),
        "hide" => Ok("hide"),
        "reject" => Ok("reject"),
        _ => Err("reply_policy must be one of off, warn, review, hide, reject".to_string()),
    }
}

fn normalize_reply_moderation_status(value: &str) -> std::result::Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approved" => Ok("approved".to_string()),
        "pending" => Ok("pending".to_string()),
        "hidden" => Ok("hidden".to_string()),
        "rejected" => Ok("rejected".to_string()),
        _ => Err("status must be approved, pending, hidden, or rejected".to_string()),
    }
}

async fn owner_moderation_reply(env: &Env, reply_id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let reply_arg = D1Type::Text(reply_id);
    let row = db
        .prepare(
            r#"
            SELECT id, post_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
                   content, published_at, created_at, moderation_status, moderation_score,
                   moderation_flags, moderation_checked_at, ai_moderation_result, hidden
            FROM replies
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&reply_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(row.map(shape_owner_moderation_reply))
}

fn shape_owner_moderation_reply(row: Map<String, Value>) -> Map<String, Value> {
    let mut item = row.clone();
    let flags = string_vec_json_field(Some(&row), "moderation_flags");
    item.insert(
        "moderation_flags".to_string(),
        Value::Array(flags.into_iter().map(Value::String).collect()),
    );
    if let Some(raw) = string_field(Some(&row), "ai_moderation_result") {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            item.insert("ai_moderation".to_string(), value);
        }
    }
    item
}

async fn owner_blocks(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, actor_id, blocked_domain, reason, created_at
        FROM blocks
        ORDER BY created_at DESC
        LIMIT 80
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_allowlist(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT host, note, enabled, created_at, updated_at
        FROM federation_allowlist
        ORDER BY host ASC
        LIMIT 120
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_followers(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox,
               status, created_at, updated_at
        FROM followers
        ORDER BY
          CASE status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
          updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_set_follower_status(
    env: &Env,
    follower_actor_id: &str,
    status: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if follower_actor_id.is_empty() {
        return Err("follower_actor_id is required".to_string());
    }
    if !matches!(status, "approved" | "pending" | "rejected") {
        return Err("status must be approved, pending, or rejected".to_string());
    }

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let local_actor_arg = D1Type::Text(&local_actor.id);
    let follower_arg = D1Type::Text(follower_actor_id);
    let existing = db
        .prepare(
            r#"
            SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox, status
            FROM followers
            WHERE actor_id = ?1 AND follower_actor_id = ?2
            LIMIT 1
            "#,
        )
        .bind_refs([&local_actor_arg, &follower_arg])
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    let Some(existing) = existing else {
        return Err("follower not found".to_string());
    };

    let status_arg = D1Type::Text(status);
    db.prepare(
        r#"
        UPDATE followers
        SET status = ?1,
            updated_at = CURRENT_TIMESTAMP
        WHERE actor_id = ?2 AND follower_actor_id = ?3
        "#,
    )
    .bind_refs([&status_arg, &local_actor_arg, &follower_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let delivery_ids = if status == "approved" {
        let follow_id =
            string_field(Some(&existing), "id").unwrap_or_else(|| follower_actor_id.to_string());
        let accept_id = format!(
            "{}#accepts/{}",
            local_actor.id,
            stable_id(&follow_id).chars().take(16).collect::<String>()
        );
        let activity = serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": accept_id,
            "type": "Accept",
            "actor": local_actor.id,
            "to": [follower_actor_id],
            "object": {
                "id": follow_id,
                "type": "Follow",
                "actor": follower_actor_id,
                "object": local_actor.id,
            },
        });
        let inbox = string_field(Some(&existing), "follower_shared_inbox")
            .or_else(|| string_field(Some(&existing), "follower_inbox"));
        insert_delivery_rows(
            env,
            &accept_id,
            inbox.into_iter().collect(),
            "Accept",
            Some(activity.to_string()),
        )
        .await?
    } else {
        Vec::new()
    };

    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(response)
}

async fn owner_friends(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let local_actor = owner_local_actor(env).await?;
    let actor_arg = D1Type::Text(&local_actor.id);
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT friend_actor_id, friend_inbox, friend_shared_inbox,
               follower_since, following_since, accepted_at
        FROM friends
        WHERE local_actor_id = ?1
        ORDER BY COALESCE(accepted_at, following_since, follower_since) DESC
        LIMIT ?2
        "#,
    )
    .bind_refs([&actor_arg, &limit_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_following(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
        FROM following
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_posts(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
               atproto_uri, atproto_cid, encrypted_message, media_attachments,
               published_at, created_at, updated_at,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
        FROM posts
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_saved_posts(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT s.id, s.post_id, s.object_id, s.canonical_url,
               COALESCE(NULLIF(s.title, ''), p.name, tp.actor_display_name, 'Saved post') AS title,
               COALESCE(NULLIF(s.excerpt, ''), p.content, tp.content) AS excerpt,
               COALESCE(NULLIF(s.source, ''), 'owner') AS source,
               s.saved_at
        FROM saved_posts s
        LEFT JOIN posts p ON p.id = s.post_id OR p.id = s.object_id
        LEFT JOIN timeline_posts tp ON tp.object_id = s.object_id OR tp.object_id = s.canonical_url
        ORDER BY s.saved_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_save_post(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let post_id = optional_trimmed_body(body, &["post_id", "postId"]);
    let object_id = optional_trimmed_body(body, &["object_id", "objectId"]);
    let canonical_url = optional_trimmed_body(body, &["canonical_url", "canonicalUrl", "url"]);
    let title = optional_trimmed_body(body, &["title", "name"]);
    let excerpt = optional_trimmed_body(body, &["excerpt", "content", "summary"]);
    let source = optional_trimmed_body(body, &["source"]).unwrap_or_else(|| "owner".to_string());
    let identity = post_id
        .as_deref()
        .or(object_id.as_deref())
        .or(canonical_url.as_deref())
        .ok_or_else(|| "post_id, object_id, or canonical_url is required".to_string())?;
    let id = format!("saved-{}", stable_id(identity));
    let raw_item = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&id);
    let post_arg = post_id.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let object_arg = object_id
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let url_arg = canonical_url
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let title_arg = title.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let excerpt_arg = excerpt.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let source_arg = D1Type::Text(&source);
    let raw_arg = D1Type::Text(&raw_item);
    db.prepare(
        r#"
        INSERT INTO saved_posts (
            id, post_id, object_id, canonical_url, title, excerpt, source, raw_item, saved_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            post_id = COALESCE(excluded.post_id, saved_posts.post_id),
            object_id = COALESCE(excluded.object_id, saved_posts.object_id),
            canonical_url = COALESCE(excluded.canonical_url, saved_posts.canonical_url),
            title = COALESCE(excluded.title, saved_posts.title),
            excerpt = COALESCE(excluded.excerpt, saved_posts.excerpt),
            source = excluded.source,
            raw_item = excluded.raw_item,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([
        &id_arg,
        &post_arg,
        &object_arg,
        &url_arg,
        &title_arg,
        &excerpt_arg,
        &source_arg,
        &raw_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_saved_post_by_id(env, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "saved post was not found after save".to_string())
}

async fn owner_saved_post_by_id(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, post_id, object_id, canonical_url, title, excerpt, source, saved_at
        FROM saved_posts
        WHERE id = ?1
        LIMIT 1
        "#,
    )
    .bind_refs(&id_arg)?
    .first::<Map<String, Value>>(None)
    .await
}

async fn owner_unsave_post(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        DELETE FROM saved_posts
        WHERE id = ?1 OR post_id = ?1 OR object_id = ?1 OR canonical_url = ?1
        "#,
    )
    .bind_refs(&id_arg)?
    .run()
    .await?;
    Ok(())
}

async fn owner_post_detail(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let canonical_id = canonical_mastodon_status_id(id);
    let id_arg = D1Type::Text(&canonical_id);
    let post = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   atproto_uri, atproto_cid, encrypted_message, media_attachments,
                   published_at, created_at, updated_at, in_reply_to,
                   (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
            FROM posts
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    let Some(post) = post else {
        return Ok(None);
    };
    let replies = owner_post_replies(env, &canonical_id).await?;
    let likes = owner_post_interactions(env, &canonical_id, "like").await?;
    let boosts = owner_post_interactions(env, &canonical_id, "boost").await?;
    Ok(Some(shape_owner_post_detail(post, replies, likes, boosts)))
}

async fn owner_delete_post(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    let existing = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   atproto_uri, atproto_cid, encrypted_message, media_attachments,
                   published_at, created_at, updated_at, in_reply_to
            FROM posts
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let protocol =
        string_field(Some(&existing), "protocol").unwrap_or_else(|| "activitypub".to_string());
    let visibility = string_field(Some(&existing), "visibility").unwrap_or_default();
    let actor_id = string_field(Some(&existing), "actor_id").unwrap_or_default();
    let delivery_ids =
        if (protocol == "activitypub" || protocol == "both") && visibility != "direct" {
            let now = js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default();
            let delete_id = format!(
                "{actor_id}#deletes/{}",
                stable_id(&format!("{id}\n{now}"))
                    .chars()
                    .take(16)
                    .collect::<String>()
            );
            let activity = serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": delete_id,
                "type": "Delete",
                "actor": actor_id,
                "published": now,
                "to": [PUBLIC_COLLECTION],
                "cc": [format!("{actor_id}/followers")],
                "object": {
                    "id": id,
                    "type": "Tombstone",
                },
            });
            insert_delivery_rows(
                env,
                id,
                owner_approved_follower_inboxes(env).await?,
                "Delete",
                Some(activity.to_string()),
            )
            .await
            .map_err(worker::Error::RustError)?
        } else {
            Vec::new()
        };

    db.prepare("DELETE FROM posts WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;

    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("id".to_string(), Value::String(id.to_string()));
    response.insert("deleted".to_string(), Value::Bool(true));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(Some(response))
}

async fn owner_create_post(
    env: &Env,
    text: &str,
    visibility: &str,
    protocol: &str,
    recipients: Vec<String>,
    attachments: Vec<Value>,
    encrypt: bool,
    in_reply_to: Option<String>,
    audience_list_id: Option<String>,
    object_type: &str,
    summary: Option<String>,
    poll_options: Option<Value>,
) -> std::result::Result<Map<String, Value>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let actor = db
        .prepare("SELECT id FROM actors WHERE username = 'social' LIMIT 1")
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    let actor_id = string_field(actor.as_ref(), "id").unwrap_or_else(|| local_actor_url(env));
    let audience_list_id = audience_list_id.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    if audience_list_id.is_some() && visibility != "direct" {
        return Err("audience lists currently require direct visibility".to_string());
    }
    let list_recipients = if let Some(list_id) = audience_list_id.as_deref() {
        owner_audience_list_recipient_actors(env, list_id).await?
    } else {
        Vec::new()
    };
    let mut resolved_recipients = recipients;
    resolved_recipients.extend(list_recipients);
    resolved_recipients.sort();
    resolved_recipients.dedup();
    let direct_targets = if visibility == "direct" {
        owner_direct_delivery_targets(env, &resolved_recipients).await?
    } else {
        Vec::new()
    };
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let local_id = format!(
        "{}-{}",
        timestamp_for_local_id(&now),
        stable_id(&format!("{now}\n{text}"))
            .chars()
            .take(8)
            .collect::<String>()
    );
    let post_id = format!("{actor_id}/posts/{local_id}");
    let content_html = format!("<p>{}</p>", escape_html(text).replace('\n', "<br>"));
    let media_attachments = normalize_attachments(&attachments)?;
    if !media_attachments.is_empty()
        && matches!(&*protocol, "atproto" | "both")
        && !media_attachments
            .iter()
            .all(is_public_atproto_image_attachment)
    {
        return Err("AT Protocol media attachments must be public image uploads".to_string());
    }
    if !media_attachments.is_empty() && encrypt {
        return Err(
            "E2EE media attachments require encrypted media support and are not implemented yet"
                .to_string(),
        );
    }
    if !media_attachments.is_empty()
        && matches!(visibility, "followers" | "direct")
        && !media_attachments.iter().all(is_private_media_attachment)
    {
        return Err(
            "private and direct media attachments must use private media upload URLs".to_string(),
        );
    }

    let mut reply_target_inbox = None;
    if let Some(in_reply_to) = in_reply_to.as_deref() {
        public_https_url(in_reply_to, "in_reply_to")?;
        if !is_local_object_url(in_reply_to, &activitypub_domain(env)) {
            reply_target_inbox = Some(resolve_activitypub_object_inbox(in_reply_to).await?);
        }
    }

    let media_attachments_json = if media_attachments.is_empty() {
        None
    } else {
        Some(Value::Array(media_attachments.clone()).to_string())
    };
    let actor_arg = D1Type::Text(&actor_id);
    let post_arg = D1Type::Text(&post_id);
    let text_arg = D1Type::Text(text);
    let content_arg = D1Type::Text(&content_html);
    let object_type_arg = D1Type::Text(object_type);
    let summary_arg = summary.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let visibility_arg = D1Type::Text(visibility);
    let protocol_arg = D1Type::Text(protocol);
    let now_arg = D1Type::Text(&now);
    let media_arg = media_attachments_json
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let reply_arg = in_reply_to
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let poll_options_json = poll_options.as_ref().map(Value::to_string);
    let poll_arg = poll_options_json
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        INSERT INTO posts (
          id, actor_id, content, content_html, object_type, summary, visibility, protocol,
          published_at, media_attachments, in_reply_to, poll_options, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind_refs([
        &post_arg,
        &actor_arg,
        &text_arg,
        &content_arg,
        &object_type_arg,
        &summary_arg,
        &visibility_arg,
        &protocol_arg,
        &now_arg,
        &media_arg,
        &reply_arg,
        &poll_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let delivery_ids = if matches!(protocol, "activitypub" | "both") {
        owner_create_post_deliveries(
            env,
            &post_id,
            visibility,
            direct_targets,
            reply_target_inbox.into_iter().collect(),
        )
        .await?
    } else {
        Vec::new()
    };

    let mut response = Map::new();
    response.insert("id".to_string(), Value::String(post_id));
    response.insert("actor_id".to_string(), Value::String(actor_id));
    response.insert("content".to_string(), Value::String(text.to_string()));
    response.insert("content_html".to_string(), Value::String(content_html));
    response.insert(
        "object_type".to_string(),
        Value::String(object_type.to_string()),
    );
    response.insert(
        "summary".to_string(),
        summary.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "visibility".to_string(),
        Value::String(visibility.to_string()),
    );
    response.insert("protocol".to_string(), Value::String(protocol.to_string()));
    response.insert("published_at".to_string(), Value::String(now));
    response.insert(
        "in_reply_to".to_string(),
        in_reply_to.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "audience_list_id".to_string(),
        audience_list_id.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "poll_options".to_string(),
        poll_options_json.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "recipients".to_string(),
        Value::Array(resolved_recipients.into_iter().map(Value::String).collect()),
    );
    response.insert("attachments".to_string(), Value::Array(media_attachments));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(response)
}

async fn owner_create_post_deliveries(
    env: &Env,
    post_id: &str,
    visibility: &str,
    direct_targets: Vec<String>,
    extra_targets: Vec<String>,
) -> std::result::Result<Vec<String>, String> {
    if visibility == "direct" {
        let mut targets = direct_targets;
        targets.extend(extra_targets);
        return insert_delivery_rows(env, post_id, targets, "Create", None).await;
    }
    let mut deliveries = insert_delivery_rows(
        env,
        post_id,
        owner_approved_follower_inboxes(env)
            .await
            .map_err(|error| error.to_string())?,
        "Create",
        None,
    )
    .await?;
    deliveries.extend(insert_delivery_rows(env, post_id, extra_targets, "Create", None).await?);
    Ok(deliveries)
}

async fn owner_direct_delivery_targets(
    env: &Env,
    recipients: &[String],
) -> std::result::Result<Vec<String>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let mut inboxes = Vec::new();
    let mut missing = Vec::new();
    for recipient in recipients {
        let recipient_arg = D1Type::Text(recipient);
        let row = db
            .prepare(
                r#"
                SELECT follower_actor_id, follower_inbox
                FROM followers
                WHERE status = 'approved'
                  AND follower_actor_id = ?1
                LIMIT 1
                "#,
            )
            .bind_refs(&recipient_arg)
            .map_err(|error| error.to_string())?
            .first::<Map<String, Value>>(None)
            .await
            .map_err(|error| error.to_string())?;
        match row.and_then(|row| string_field(Some(&row), "follower_inbox")) {
            Some(inbox) => inboxes.push(inbox),
            None => missing.push(recipient.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "direct recipients must be approved followers with known inboxes: {}",
            missing.join(", ")
        ));
    }
    Ok(inboxes)
}

async fn owner_audience_lists(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let lists = db
        .prepare(
            r#"
            SELECT id, name, description, allowed_categories, created_at, updated_at
            FROM audience_lists
            ORDER BY name COLLATE NOCASE ASC, created_at DESC
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let mut shaped = Vec::new();
    for row in lists {
        let id = string_field(Some(&row), "id").unwrap_or_default();
        let member_actor_ids = owner_audience_list_member_actor_ids(env, &id).await?;
        let allowed_categories = string_vec_json_field(Some(&row), "allowed_categories");
        let mut item = Map::new();
        item.insert("id".to_string(), Value::String(id));
        item.insert("name".to_string(), string_value_or_default(&row, "name"));
        item.insert(
            "description".to_string(),
            row_value_or_null(&row, "description"),
        );
        item.insert(
            "allowed_categories".to_string(),
            Value::Array(allowed_categories.into_iter().map(Value::String).collect()),
        );
        item.insert(
            "member_actor_ids".to_string(),
            Value::Array(
                member_actor_ids
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        item.insert(
            "member_count".to_string(),
            Value::from(member_actor_ids.len() as i64),
        );
        item.insert(
            "created_at".to_string(),
            row_value_or_null(&row, "created_at"),
        );
        item.insert(
            "updated_at".to_string(),
            row_value_or_null(&row, "updated_at"),
        );
        shaped.push(item);
    }
    Ok(shaped)
}

async fn owner_audience_list(env: &Env, list_id: &str) -> Result<Option<Map<String, Value>>> {
    let lists = owner_audience_lists(env).await?;
    Ok(lists.into_iter().find(|row| {
        row.get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value == list_id)
    }))
}

async fn owner_upsert_audience_list(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let name = body_string_any(body, &["name"]).ok_or_else(|| "name is required".to_string())?;
    let description = body.get("description").and_then(optional_body_string);
    let allowed_categories = normalize_sensitive_categories(body_string_array_any(
        body,
        &["allowed_categories", "allowedCategories"],
    ));
    let member_actor_ids = {
        let mut unique = Vec::new();
        for value in body_string_array_any(body, &["member_actor_ids", "memberActorIds"]) {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() || unique.contains(&trimmed) {
                continue;
            }
            unique.push(trimmed);
        }
        unique
    };
    let id = body_string_any(body, &["id"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            let created = js_sys::Date::new_0()
                .to_iso_string()
                .as_string()
                .unwrap_or_default();
            format!("audience-{}-{}", stable_id(&name), stable_id(&created))
        });

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(id.as_str());
    let name_arg = D1Type::Text(name.as_str());
    let description_arg = description
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let allowed_categories_json =
        serde_json::to_string(&allowed_categories).map_err(|error| error.to_string())?;
    let categories_arg = D1Type::Text(allowed_categories_json.as_str());

    db.prepare(
        r#"
        INSERT INTO audience_lists (id, name, description, allowed_categories, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          description = excluded.description,
          allowed_categories = excluded.allowed_categories,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([&id_arg, &name_arg, &description_arg, &categories_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    db.prepare("DELETE FROM audience_list_members WHERE list_id = ?1")
        .bind_refs(&id_arg)
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;

    for actor_id in &member_actor_ids {
        let actor_arg = D1Type::Text(actor_id.as_str());
        db.prepare(
            r#"
            INSERT INTO audience_list_members (list_id, actor_id, created_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            "#,
        )
        .bind_refs([&id_arg, &actor_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    }

    owner_audience_list(env, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "failed to load saved audience list".to_string())
}

async fn owner_delete_audience_list(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("DELETE FROM audience_list_members WHERE list_id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    db.prepare("DELETE FROM audience_lists WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_audience_list_member_actor_ids(env: &Env, list_id: &str) -> Result<Vec<String>> {
    let db = env.d1("DB")?;
    let list_arg = D1Type::Text(list_id);
    let rows = db
        .prepare(
            r#"
            SELECT actor_id
            FROM audience_list_members
            WHERE list_id = ?1
            ORDER BY actor_id ASC
            "#,
        )
        .bind_refs(&list_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|row| string_field(Some(&row), "actor_id"))
        .collect())
}

async fn owner_audience_list_recipient_actors(
    env: &Env,
    list_id: &str,
) -> std::result::Result<Vec<String>, String> {
    let members = owner_audience_list_member_actor_ids(env, list_id)
        .await
        .map_err(|error| error.to_string())?;
    if members.is_empty() {
        return Err("selected audience list has no members".to_string());
    }
    Ok(members)
}

async fn owner_post_replies(env: &Env, id: &str) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               content, published_at, created_at, visibility
        FROM replies
        WHERE post_id = ?1 AND (hidden IS NULL OR hidden = 0)
        ORDER BY published_at ASC
        "#,
    )
    .bind_refs(&id_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_post_interactions(
    env: &Env,
    id: &str,
    interaction_type: &str,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    let type_arg = D1Type::Text(interaction_type);
    db.prepare(
        r#"
        SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               object_url, created_at
        FROM interactions
        WHERE (post_id = ?1 OR object_url = ?1) AND type = ?2
        ORDER BY created_at DESC
        "#,
    )
    .bind_refs([&id_arg, &type_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

fn shape_owner_post_detail(
    post: Map<String, Value>,
    replies: Vec<Map<String, Value>>,
    likes: Vec<Map<String, Value>>,
    boosts: Vec<Map<String, Value>>,
) -> Map<String, Value> {
    let mut detail = Map::new();
    detail.insert("id".to_string(), row_value_or_null(&post, "id"));
    detail.insert("actor_id".to_string(), row_value_or_null(&post, "actor_id"));
    detail.insert("title".to_string(), row_value_or_null(&post, "name"));
    detail.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    detail.insert(
        "content_html".to_string(),
        row_value_or_null(&post, "content_html"),
    );
    detail.insert(
        "visibility".to_string(),
        Value::String(title_visibility(
            string_field(Some(&post), "visibility").as_deref(),
        )),
    );
    detail.insert(
        "protocol".to_string(),
        Value::String(title_protocol(
            string_field(Some(&post), "protocol").as_deref(),
        )),
    );
    detail.insert(
        "encrypted".to_string(),
        Value::Bool(non_empty_value(&post, "encrypted_message").is_some()),
    );
    detail.insert(
        "attachments".to_string(),
        Value::Array(parse_attachment_array(post.get("media_attachments"))),
    );
    detail.insert(
        "in_reply_to".to_string(),
        row_value_or_null(&post, "in_reply_to"),
    );
    detail.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    detail.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    detail.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    detail.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    detail.insert(
        "replies".to_string(),
        Value::Array(replies.into_iter().map(Value::Object).collect()),
    );
    detail.insert(
        "likes".to_string(),
        Value::Array(likes.into_iter().map(Value::Object).collect()),
    );
    detail.insert(
        "boosts".to_string(),
        Value::Array(boosts.into_iter().map(Value::Object).collect()),
    );
    detail
}

fn title_visibility(value: Option<&str>) -> String {
    match value {
        Some("public") => "Public",
        Some("unlisted") => "Unlisted",
        Some("direct") => "Direct",
        _ => "Followers",
    }
    .to_string()
}

fn title_protocol(value: Option<&str>) -> String {
    match value {
        Some("atproto") => "AtProto",
        Some("both") => "Both",
        _ => "ActivityPub",
    }
    .to_string()
}

fn parse_attachment_array(value: Option<&Value>) -> Vec<Value> {
    let parsed = match value {
        Some(Value::Array(items)) => Some(items.clone()),
        Some(Value::String(text)) if !text.trim().is_empty() => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| match value {
                Value::Array(items) => Some(items),
                _ => None,
            }),
        _ => None,
    };
    parsed
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            item.as_object()
                .and_then(|object| object.get("url"))
                .and_then(Value::as_str)
                .is_some()
        })
        .collect()
}

async fn owner_home_timeline(
    env: &Env,
    limit: i32,
    include_replies: bool,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let include_replies_arg = D1Type::Integer(if include_replies { 1 } else { 0 });
    db.prepare(
        r#"
        SELECT id, object_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               content, content_html, visibility, in_reply_to, published_at, updated_at,
               protocol, created_at,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = timeline_posts.object_id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'boost') AS boost_count
        FROM timeline_posts
        WHERE deleted_at IS NULL
          AND (?2 = 1 OR in_reply_to IS NULL OR in_reply_to = '')
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs([&limit_arg, &include_replies_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_notifications(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT n.id, n.type, n.actor_id, n.actor_username, n.actor_display_name,
               n.actor_avatar_url, n.post_id, n.activity_id, n.content, n.read,
               n.created_at,
               p.id AS context_post_id,
               p.content AS context_post_content,
               p.content_html AS context_post_content_html,
               p.visibility AS context_post_visibility,
               p.protocol AS context_post_protocol,
               p.published_at AS context_post_published_at
        FROM notifications n
        LEFT JOIN posts p ON p.id = n.post_id
        ORDER BY n.created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_mark_notification_read(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("UPDATE notifications SET read = 1 WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_deliveries(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, post_id, target_type, target_url, protocol, status, retry_count,
               last_attempt_at, error_message, activity_type, created_at, delivered_at
        FROM deliveries
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

fn owner_delivery_action_path(path: &str) -> Option<(String, &'static str)> {
    let rest = path.strip_prefix("/deliveries/")?;
    if let Some(id) = rest.strip_suffix("/retry") {
        return (!id.is_empty()).then(|| (decode_component(id), "retry"));
    }
    if let Some(id) = rest.strip_suffix("/cancel") {
        return (!id.is_empty()).then(|| (decode_component(id), "cancel"));
    }
    None
}

async fn owner_delivery_by_id(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, post_id, target_type, target_url, protocol, status, retry_count,
               last_attempt_at, error_message, activity_type, created_at, delivered_at
        FROM deliveries
        WHERE id = ?1
        LIMIT 1
        "#,
    )
    .bind_refs(&id_arg)?
    .first::<Map<String, Value>>(None)
    .await
}

async fn owner_delivery_rows_for_post(env: &Env, post_id: &str) -> Result<Vec<Map<String, Value>>> {
    let post_arg = D1Type::Text(post_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, post_id, target_type, target_url, protocol, status, retry_count,
                   last_attempt_at, error_message, activity_type, created_at, delivered_at
            FROM deliveries
            WHERE post_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind_refs(&post_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()
}

async fn owner_update_delivery_status(
    env: &Env,
    id: &str,
    action: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if id.trim().is_empty() {
        return Err("delivery id is required".to_string());
    }
    let Some(existing) = owner_delivery_by_id(env, id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Err("delivery not found".to_string());
    };
    let status = string_field(Some(&existing), "status").unwrap_or_default();
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(id);
    match action {
        "retry" => {
            if status == "delivered" {
                return Err("delivered deliveries cannot be retried".to_string());
            }
            db.prepare(
                r#"
                UPDATE deliveries
                SET status = 'queued',
                    retry_count = COALESCE(retry_count, 0) + 1,
                    error_message = NULL,
                    delivered_at = NULL
                WHERE id = ?1
                "#,
            )
            .bind_refs(&id_arg)
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        "cancel" => {
            if status == "delivered" {
                return Err("delivered deliveries cannot be cancelled".to_string());
            }
            db.prepare(
                r#"
                UPDATE deliveries
                SET status = 'failed',
                    error_message = 'cancelled by owner',
                    delivered_at = NULL
                WHERE id = ?1
                "#,
            )
            .bind_refs(&id_arg)
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        _ => return Err("unsupported delivery action".to_string()),
    }
    owner_delivery_by_id(env, id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "delivery not found after update".to_string())
}

async fn owner_direct_messages(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, conversation_id, sender_id, content, published_at, created_at
        FROM direct_messages
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_e2ee_messages(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let local_actor = owner_local_actor(env).await?;
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT m.id, m.conversation_id, m.sender_actor_id, m.sender_device_id,
                   m.ciphertext, m.aad, m.created_at, c.participants, c.protocol
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            ORDER BY m.created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let mut items = Vec::new();
    for row in rows {
        items.push(owner_e2ee_message_row(env, &local_actor.id, row).await?);
    }
    Ok(items)
}

async fn owner_send_e2ee_message(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let recipient_actor_id = public_https_url(
        &body_string_any(
            body,
            &["recipient_actor_id", "recipientActorId", "recipient"],
        )
        .ok_or("recipientActorId is required")?,
        "recipientActorId",
    )?;
    if recipient_actor_id == local_actor.id {
        return Err("recipientActorId must be a remote actor".to_string());
    }
    let sender_device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["sender_device_id", "senderDeviceId"])
            .ok_or("senderDeviceId is required")?,
    )?;
    let encrypted_message = body
        .get("dais_encrypted_message")
        .or_else(|| body.get("daisEncryptedMessage"))
        .or_else(|| body.get("encrypted_message"))
        .or_else(|| body.get("encryptedMessage"))
        .cloned()
        .ok_or("encryptedMessage or daisEncryptedMessage is required")?;
    let (envelope_field, protocol) = validate_owner_e2ee_payload(&encrypted_message)?;
    if protocol == "mls-rfc9420" {
        let envelope_sender = encrypted_message
            .get("senderDeviceId")
            .and_then(Value::as_str)
            .ok_or("daisEncryptedMessage.senderDeviceId is required")?;
        if normalize_e2ee_device_id(envelope_sender)? != sender_device_id {
            return Err(
                "senderDeviceId must match daisEncryptedMessage.senderDeviceId".to_string(),
            );
        }
    }
    let fallback_content = body_string_any(body, &["fallback_content", "fallbackContent"])
        .unwrap_or_else(|| "Encrypted message. Open in a dais client to decrypt.".to_string());
    if fallback_content.len() > 512 {
        return Err("fallbackContent is too long".to_string());
    }
    let attachments = normalize_encrypted_media_attachments(
        &body
            .get("attachments")
            .or_else(|| body.get("media_attachments"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )?;
    if !attachments.is_empty() && protocol != "dais-mls-v1" {
        return Err(
            "encrypted media attachments are currently supported only for encryptedMessage v1"
                .to_string(),
        );
    }

    let local_device =
        owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &sender_device_id)
            .await
            .map_err(|error| error.to_string())?;
    match local_device
        .as_ref()
        .and_then(|row| string_field(Some(row), "status"))
        .as_deref()
    {
        Some("active") => {}
        _ => return Err("senderDeviceId is not an active local E2EE device".to_string()),
    }

    let recipient_device_id = body_string_any(body, &["recipient_device_id", "recipientDeviceId"]);
    owner_require_trusted_e2ee_peer(env, &recipient_actor_id, recipient_device_id.as_deref())
        .await?;
    let inbox = owner_e2ee_inbox_for_actor(env, &recipient_actor_id).await?;

    let mut participants = vec![local_actor.id.clone(), recipient_actor_id.clone()];
    participants.sort();
    let participants_json =
        serde_json::to_string(&participants).map_err(|error| error.to_string())?;
    let conversation_id = format!("e2ee-conversation-{}", stable_id(&participants.join("\n")));
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let message_id = format!(
        "{}/e2ee/messages/{}-{}",
        local_actor.id,
        timestamp_for_local_id(&now),
        stable_id(&serde_json::to_string(&encrypted_message).unwrap_or_default())
    );
    let aad = serde_json::json!({
        "recipientActorId": recipient_actor_id,
        "fallbackContent": fallback_content,
        "attachments": attachments.clone(),
    });
    let aad_json = serde_json::to_string(&aad).map_err(|error| error.to_string())?;
    let ciphertext_json =
        serde_json::to_string(&encrypted_message).map_err(|error| error.to_string())?;

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let conversation_arg = D1Type::Text(&conversation_id);
    let participants_arg = D1Type::Text(&participants_json);
    db.prepare(
        r#"
        INSERT INTO e2ee_conversations (id, protocol, participants, created_at, updated_at)
        VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
        ON CONFLICT(id) DO UPDATE SET protocol = excluded.protocol, updated_at = datetime('now')
        "#,
    )
    .bind_refs(&[conversation_arg, D1Type::Text(protocol), participants_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let id_arg = D1Type::Text(&message_id);
    let conversation_arg = D1Type::Text(&conversation_id);
    let sender_actor_arg = D1Type::Text(&local_actor.id);
    let sender_device_arg = D1Type::Text(&sender_device_id);
    let ciphertext_arg = D1Type::Text(&ciphertext_json);
    let aad_arg = D1Type::Text(&aad_json);
    db.prepare(
        r#"
        INSERT INTO e2ee_messages (
            id, conversation_id, sender_actor_id, sender_device_id, ciphertext, aad, created_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, datetime('now')
        )
        "#,
    )
    .bind_refs(&[
        id_arg,
        conversation_arg,
        sender_actor_arg,
        sender_device_arg,
        ciphertext_arg,
        aad_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    if protocol == "mls-rfc9420" {
        persist_mls_message_metadata(
            env,
            &message_id,
            &conversation_id,
            &encrypted_message,
            &local_actor.id,
            &sender_device_id,
            &now,
        )
        .await?;
    }

    let mut note = serde_json::json!({
        "id": message_id,
        "type": "Note",
        "attributedTo": local_actor.id,
        "to": [recipient_actor_id],
        "published": now,
        "content": fallback_content,
        "daisE2ee": {
            "v": if protocol == "mls-rfc9420" { 2 } else { 1 },
            "protocol": protocol,
            "senderDeviceId": sender_device_id,
        },
    });
    if let Some(object) = note.as_object_mut() {
        object.insert(envelope_field.to_string(), encrypted_message.clone());
        if !attachments.is_empty() {
            object.insert("attachment".to_string(), Value::Array(attachments.clone()));
        }
        if protocol == "mls-rfc9420" {
            if let Some(group_id) = encrypted_message.get("groupId").cloned() {
                object
                    .get_mut("daisE2ee")
                    .and_then(Value::as_object_mut)
                    .map(|dais| dais.insert("groupId".to_string(), group_id));
            }
            if let Some(epoch) = encrypted_message.get("epoch").cloned() {
                object
                    .get_mut("daisE2ee")
                    .and_then(Value::as_object_mut)
                    .map(|dais| dais.insert("epoch".to_string(), epoch));
            }
        }
    }

    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{message_id}#create"),
        "type": "Create",
        "actor": local_actor.id,
        "published": now,
        "to": [recipient_actor_id],
        "object": note
    });
    let delivery_ids = insert_delivery_rows(
        env,
        &message_id,
        vec![inbox],
        "Create",
        Some(activity.to_string()),
    )
    .await?;
    let mut message = owner_e2ee_message_by_id(env, &local_actor.id, &message_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "message not found after insert".to_string())?;
    message.insert(
        "delivery_ids".to_string(),
        Value::Array(
            delivery_ids
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    let mut result = Map::new();
    result.insert("ok".to_string(), Value::Bool(true));
    result.insert("message".to_string(), Value::Object(message));
    result.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(result)
}

async fn owner_delete_e2ee_message(env: &Env, message_id: &str) -> Result<bool> {
    let local_actor = owner_local_actor(env).await?;
    let db = env.d1("DB")?;
    let message_arg = D1Type::Text(message_id);
    let Some(row) = db
        .prepare(
            r#"
            SELECT m.conversation_id, c.participants
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            WHERE m.id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&message_arg)?
        .first::<Map<String, Value>>(None)
        .await?
    else {
        return Ok(false);
    };
    let participants = string_vec_json_field(Some(&row), "participants");
    if !participants.iter().any(|actor| actor == &local_actor.id) {
        return Ok(false);
    }
    let conversation_id = string_field(Some(&row), "conversation_id").unwrap_or_default();

    db.prepare("DELETE FROM e2ee_mls_message_metadata WHERE message_id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;
    db.prepare("DELETE FROM e2ee_messages WHERE id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;
    db.prepare("DELETE FROM deliveries WHERE post_id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;

    if !conversation_id.is_empty() {
        let conversation_arg = D1Type::Text(&conversation_id);
        let remaining = db
            .prepare("SELECT id FROM e2ee_messages WHERE conversation_id = ?1 LIMIT 1")
            .bind_refs(&conversation_arg)?
            .first::<Map<String, Value>>(None)
            .await?;
        if remaining.is_none() {
            db.prepare("DELETE FROM e2ee_conversations WHERE id = ?1")
                .bind_refs(&conversation_arg)?
                .run()
                .await?;
        }
    }
    Ok(true)
}

async fn owner_e2ee_message_by_id(
    env: &Env,
    local_actor_id: &str,
    message_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let message_arg = D1Type::Text(message_id);
    let row = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT m.id, m.conversation_id, m.sender_actor_id, m.sender_device_id,
                   m.ciphertext, m.aad, m.created_at, c.participants, c.protocol
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            WHERE m.id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&message_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    match row {
        Some(row) => Ok(Some(
            owner_e2ee_message_row(env, local_actor_id, row).await?,
        )),
        None => Ok(None),
    }
}

async fn owner_e2ee_message_row(
    env: &Env,
    local_actor_id: &str,
    row: Map<String, Value>,
) -> Result<Map<String, Value>> {
    let message_id = string_field(Some(&row), "id").unwrap_or_default();
    let aad = string_field(Some(&row), "aad")
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let encrypted_message = string_field(Some(&row), "ciphertext")
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let protocol = string_field(Some(&row), "protocol")
        .or_else(|| {
            aad.get("e2eeProtocol")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "dais-mls-v1".to_string());
    let participants = string_vec_json_field(Some(&row), "participants");
    let recipient_actor_id = aad
        .get("recipientActorId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            participants
                .iter()
                .find(|actor_id| actor_id.as_str() != local_actor_id)
                .cloned()
        });
    let fallback_content = aad
        .get("fallbackContent")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let attachments = aad
        .get("attachments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let delivery_statuses = owner_delivery_rows_for_post(env, &message_id).await?;
    let mut item = Map::new();
    item.insert("id".to_string(), string_value_or_default(&row, "id"));
    item.insert(
        "conversation_id".to_string(),
        string_value_or_default(&row, "conversation_id"),
    );
    item.insert(
        "sender_actor_id".to_string(),
        string_value_or_default(&row, "sender_actor_id"),
    );
    item.insert(
        "sender_device_id".to_string(),
        string_value_or_default(&row, "sender_device_id"),
    );
    item.insert(
        "recipient_actor_id".to_string(),
        recipient_actor_id.map(Value::String).unwrap_or(Value::Null),
    );
    item.insert("e2ee_protocol".to_string(), Value::String(protocol.clone()));
    if protocol == "mls-rfc9420" {
        item.insert(
            "dais_encrypted_message".to_string(),
            encrypted_message.clone(),
        );
        item.insert("encrypted_message".to_string(), Value::Null);
        item.insert(
            "mls_group_id".to_string(),
            encrypted_message
                .get("groupId")
                .cloned()
                .unwrap_or(Value::Null),
        );
        item.insert(
            "mls_epoch".to_string(),
            encrypted_message
                .get("epoch")
                .cloned()
                .unwrap_or(Value::Null),
        );
    } else {
        item.insert("encrypted_message".to_string(), encrypted_message);
        item.insert("dais_encrypted_message".to_string(), Value::Null);
        item.insert("mls_group_id".to_string(), Value::Null);
        item.insert("mls_epoch".to_string(), Value::Null);
    }
    item.insert(
        "fallback_content".to_string(),
        fallback_content.map(Value::String).unwrap_or(Value::Null),
    );
    item.insert("attachments".to_string(), Value::Array(attachments));
    item.insert(
        "delivery_ids".to_string(),
        Value::Array(
            delivery_statuses
                .iter()
                .filter_map(|delivery| string_field(Some(delivery), "id"))
                .map(Value::String)
                .collect(),
        ),
    );
    item.insert(
        "delivery_statuses".to_string(),
        Value::Array(delivery_statuses.into_iter().map(Value::Object).collect()),
    );
    item.insert(
        "created_at".to_string(),
        row_value_or_null(&row, "created_at"),
    );
    Ok(item)
}

async fn public_e2ee_devices(env: &Env, actor_id: &str) -> Result<Vec<Value>> {
    let actor_arg = D1Type::Text(actor_id);
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT device_id, display_name, protocol, credential, key_package, fingerprint, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1 AND status = 'active'
            ORDER BY updated_at DESC, device_id ASC
            "#,
        )
        .bind_refs(&actor_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut device = Map::new();
            insert_if_string(&mut device, "deviceId", row.get("device_id"));
            insert_if_string(&mut device, "displayName", row.get("display_name"));
            insert_if_string(&mut device, "protocol", row.get("protocol"));
            insert_if_string(&mut device, "credential", row.get("credential"));
            insert_if_string(&mut device, "keyPackage", row.get("key_package"));
            insert_if_string(&mut device, "fingerprint", row.get("fingerprint"));
            insert_if_string(&mut device, "updatedAt", row.get("updated_at"));
            Value::Object(device)
        })
        .collect())
}

async fn owner_e2ee_devices(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let local_actor = owner_local_actor(env).await?;
    let actor_arg = D1Type::Text(&local_actor.id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, status, created_at, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1
            ORDER BY updated_at DESC, device_id ASC
            "#,
        )
        .bind_refs(&actor_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()
}

async fn owner_upsert_e2ee_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let display_name = body_string_any(body, &["display_name", "displayName"]);
    let protocol = normalize_e2ee_protocol(
        body_string_any(body, &["protocol"])
            .unwrap_or_else(|| "dais-mls-v1".to_string())
            .as_str(),
    )?;
    let credential = required_e2ee_material(body, &["credential", "identityKey"], "credential")?;
    let key_package = required_e2ee_material(body, &["key_package", "keyPackage"], "keyPackage")?;
    validate_e2ee_device_material(&protocol, &credential, &key_package)?;
    let fingerprint = e2ee_device_fingerprint(&credential, &key_package);
    let row_id = format!(
        "e2ee-device-{}",
        stable_id(&format!("{}\n{}", local_actor.id, device_id))
    );
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&row_id);
    let actor_arg = D1Type::Text(&local_actor.id);
    let device_arg = D1Type::Text(&device_id);
    let display_arg = display_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let protocol_arg = D1Type::Text(&protocol);
    let credential_arg = D1Type::Text(&credential);
    let key_package_arg = D1Type::Text(&key_package);
    let fingerprint_arg = D1Type::Text(&fingerprint);
    db.prepare(
        r#"
        INSERT INTO e2ee_devices (
            id, actor_id, device_id, display_name, protocol, credential, key_package,
            fingerprint, status, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', datetime('now'), datetime('now')
        )
        ON CONFLICT(actor_id, device_id) DO UPDATE SET
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            credential = excluded.credential,
            key_package = excluded.key_package,
            fingerprint = excluded.fingerprint,
            status = 'active',
            updated_at = datetime('now')
        "#,
    )
    .bind_refs(&[
        id_arg,
        actor_arg,
        device_arg,
        display_arg,
        protocol_arg,
        credential_arg,
        key_package_arg,
        fingerprint_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "device not found after upsert".to_string())
}

async fn owner_revoke_e2ee_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let actor_arg = D1Type::Text(&local_actor.id);
    let device_arg = D1Type::Text(&device_id);
    env.d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            UPDATE e2ee_devices
            SET status = 'revoked', updated_at = datetime('now')
            WHERE actor_id = ?1 AND device_id = ?2
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "device not found".to_string())
}

async fn owner_e2ee_peer_devices(env: &Env) -> Result<Vec<Map<String, Value>>> {
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
            FROM e2ee_peer_devices
            ORDER BY last_seen_at DESC, actor_id ASC, device_id ASC
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()
}

async fn owner_discover_e2ee_peer_devices(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let target = body_string_any(body, &["actor_id", "actorId", "actor", "target"])
        .ok_or("actorId is required")?;
    let actor_url = activitypub_actor_url_for_target(&target).await?;
    let actor = match fetch_activitypub_json(&actor_url, "actor").await {
        Ok(actor) => actor,
        Err(unsigned_error)
            if should_retry_signed_fetch(&unsigned_error) && local_actor.can_sign() =>
        {
            fetch_activitypub_json_signed(&actor_url, "actor", &local_actor)
                .await
                .map_err(|signed_error| {
                    format!("{unsigned_error}; signed retry failed: {signed_error}")
                })?
        }
        Err(error) => return Err(error),
    };
    let actor_id = actor
        .get("id")
        .and_then(optional_body_string)
        .unwrap_or(actor_url);
    public_https_url(&actor_id, "actorId")?;
    let devices = actor
        .get("daisE2ee")
        .and_then(|value| value.get("devices"))
        .and_then(Value::as_array)
        .ok_or("actor does not publish daisE2ee.devices")?;
    if devices.is_empty() {
        return Err("actor publishes no E2EE devices".to_string());
    }

    let mut rows = Vec::new();
    for device in devices {
        let Some(device) = device.as_object() else {
            return Err("daisE2ee device must be an object".to_string());
        };
        let mut peer = Map::new();
        peer.insert("actorId".to_string(), Value::String(actor_id.clone()));
        copy_e2ee_device_field(device, &mut peer, "deviceId", "deviceId")?;
        copy_e2ee_device_field(device, &mut peer, "credential", "credential")?;
        copy_e2ee_device_field(device, &mut peer, "keyPackage", "keyPackage")?;
        copy_optional_e2ee_device_field(device, &mut peer, "displayName", "displayName");
        copy_optional_e2ee_device_field(device, &mut peer, "protocol", "protocol");
        copy_optional_e2ee_device_field(device, &mut peer, "fingerprint", "fingerprint");
        let row =
            owner_upsert_peer_device_with_trust(env, &Value::Object(peer), "untrusted").await?;
        rows.push(Value::Object(row));
    }

    let mut result = Map::new();
    result.insert("actor_id".to_string(), Value::String(actor_id));
    result.insert("items".to_string(), Value::Array(rows));
    Ok(result)
}

fn copy_e2ee_device_field(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
) -> std::result::Result<(), String> {
    let value = source
        .get(source_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("daisE2ee device missing {source_key}"))?;
    target.insert(target_key.to_string(), Value::String(value.to_string()));
    Ok(())
}

fn copy_optional_e2ee_device_field(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
) {
    if let Some(value) = source
        .get(source_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        target.insert(target_key.to_string(), Value::String(value.to_string()));
    }
}

async fn owner_trust_e2ee_peer_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    owner_upsert_peer_device_with_trust(env, body, "trusted").await
}

async fn owner_revoke_e2ee_peer_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let actor_id = public_https_url(
        &body_string_any(body, &["actor_id", "actorId", "actor"]).ok_or("actorId is required")?,
        "actorId",
    )?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let actor_arg = D1Type::Text(&actor_id);
    let device_arg = D1Type::Text(&device_id);
    env.d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            UPDATE e2ee_peer_devices
            SET trust_state = 'revoked', revoked_at = datetime('now'), last_seen_at = datetime('now')
            WHERE actor_id = ?1 AND device_id = ?2
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "peer device not found".to_string())
}

async fn owner_upsert_peer_device_with_trust(
    env: &Env,
    body: &Value,
    trust_state: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let actor_id = public_https_url(
        &body_string_any(body, &["actor_id", "actorId", "actor"]).ok_or("actorId is required")?,
        "actorId",
    )?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let display_name = body_string_any(body, &["display_name", "displayName"]);
    let protocol = normalize_e2ee_protocol(
        body_string_any(body, &["protocol"])
            .unwrap_or_else(|| "dais-mls-v1".to_string())
            .as_str(),
    )?;
    let credential = required_e2ee_material(body, &["credential", "identityKey"], "credential")?;
    let key_package = required_e2ee_material(body, &["key_package", "keyPackage"], "keyPackage")?;
    validate_e2ee_device_material(&protocol, &credential, &key_package)?;
    let fingerprint = body_string_any(body, &["fingerprint"])
        .map(|value| normalize_e2ee_fingerprint(&value))
        .transpose()?
        .unwrap_or_else(|| e2ee_device_fingerprint(&credential, &key_package));
    if fingerprint != e2ee_device_fingerprint(&credential, &key_package) {
        return Err("fingerprint does not match credential and keyPackage".to_string());
    }
    let existing = owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?;
    let existing_fingerprint = existing
        .as_ref()
        .and_then(|row| string_field(Some(row), "fingerprint"));
    let existing_trust_state = existing
        .as_ref()
        .and_then(|row| string_field(Some(row), "trust_state"));
    let effective_trust_state = peer_trust_state_after_material_update(
        existing_fingerprint.as_deref(),
        existing_trust_state.as_deref(),
        trust_state,
        &fingerprint,
    );
    let row_id = format!(
        "e2ee-peer-{}",
        stable_id(&format!("{}\n{}", actor_id, device_id))
    );
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&row_id);
    let actor_arg = D1Type::Text(&actor_id);
    let device_arg = D1Type::Text(&device_id);
    let display_arg = display_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let protocol_arg = D1Type::Text(&protocol);
    let credential_arg = D1Type::Text(&credential);
    let key_package_arg = D1Type::Text(&key_package);
    let fingerprint_arg = D1Type::Text(&fingerprint);
    let trust_arg = D1Type::Text(effective_trust_state);
    db.prepare(
        r#"
        INSERT INTO e2ee_peer_devices (
            id, actor_id, device_id, display_name, protocol, credential, key_package,
            fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), datetime('now'),
            CASE WHEN ?9 = 'trusted' THEN datetime('now') ELSE NULL END, NULL
        )
        ON CONFLICT(actor_id, device_id) DO UPDATE SET
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            credential = excluded.credential,
            key_package = excluded.key_package,
            fingerprint = excluded.fingerprint,
            trust_state = excluded.trust_state,
            last_seen_at = datetime('now'),
            trusted_at = CASE
                WHEN excluded.trust_state = 'trusted' THEN datetime('now')
                WHEN e2ee_peer_devices.fingerprint != excluded.fingerprint THEN NULL
                ELSE trusted_at
            END,
            revoked_at = CASE WHEN excluded.trust_state = 'revoked' THEN datetime('now') ELSE NULL END
        "#,
    )
    .bind_refs(&[
        id_arg,
        actor_arg,
        device_arg,
        display_arg,
        protocol_arg,
        credential_arg,
        key_package_arg,
        fingerprint_arg,
        trust_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "peer device not found after upsert".to_string())
}

async fn owner_e2ee_device_by_actor_and_device(
    env: &Env,
    actor_id: &str,
    device_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let actor_arg = D1Type::Text(actor_id);
    let device_arg = D1Type::Text(device_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, status, created_at, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1 AND device_id = ?2
            LIMIT 1
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])?
        .first::<Map<String, Value>>(None)
        .await
}

async fn owner_e2ee_peer_device_by_actor_and_device(
    env: &Env,
    actor_id: &str,
    device_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let actor_arg = D1Type::Text(actor_id);
    let device_arg = D1Type::Text(device_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
            FROM e2ee_peer_devices
            WHERE actor_id = ?1 AND device_id = ?2
            LIMIT 1
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])?
        .first::<Map<String, Value>>(None)
        .await
}

async fn owner_require_trusted_e2ee_peer(
    env: &Env,
    actor_id: &str,
    device_id: Option<&str>,
) -> std::result::Result<(), String> {
    if let Some(device_id) = device_id {
        let device_id = normalize_e2ee_device_id(device_id)?;
        let Some(peer) = owner_e2ee_peer_device_by_actor_and_device(env, actor_id, &device_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Err("recipientDeviceId is not known".to_string());
        };
        if string_field(Some(&peer), "trust_state").as_deref() == Some("trusted") {
            return Ok(());
        }
        return Err("recipientDeviceId is not trusted".to_string());
    }

    let actor_arg = D1Type::Text(actor_id);
    let trusted = env
        .d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            SELECT id
            FROM e2ee_peer_devices
            WHERE actor_id = ?1 AND trust_state = 'trusted'
            LIMIT 1
            "#,
        )
        .bind_refs(&actor_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    trusted
        .map(|_| ())
        .ok_or_else(|| "recipient has no trusted E2EE device".to_string())
}

async fn owner_e2ee_inbox_for_actor(
    env: &Env,
    actor_id: &str,
) -> std::result::Result<String, String> {
    let actor_arg = D1Type::Text(actor_id);
    let row = env
        .d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            SELECT inbox FROM (
                SELECT follower_inbox AS inbox, 0 AS rank
                FROM followers
                WHERE follower_actor_id = ?1 AND status = 'approved'
                UNION ALL
                SELECT target_inbox AS inbox, 1 AS rank
                FROM following
                WHERE target_actor_id = ?1 AND status IN ('accepted', 'pending')
            )
            WHERE inbox IS NOT NULL AND inbox <> ''
            ORDER BY rank ASC
            LIMIT 1
            "#,
        )
        .bind_refs(&actor_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(inbox) = row
        .as_ref()
        .and_then(|row| string_field(Some(row), "inbox"))
    {
        Ok(inbox)
    } else {
        async_resolve_e2ee_actor_inbox(env, actor_id).await
    }
}

async fn async_resolve_e2ee_actor_inbox(
    env: &Env,
    actor_id: &str,
) -> std::result::Result<String, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let actor = resolve_activitypub_actor_for_local(actor_id, &local_actor).await?;
    let inbox = actor.shared_inbox.unwrap_or(actor.inbox);
    if inbox.trim().is_empty() {
        return Err("recipient actor does not expose an inbox".to_string());
    }
    public_https_url(&inbox, "recipient inbox")
}

#[derive(Clone)]
struct OwnerSearchFlags {
    include_local: bool,
    include_public: bool,
    confirm_public_sensitive: bool,
    public_options: OwnerPublicSearchOptions,
}

fn owner_search_flags(url: &worker::Url) -> OwnerSearchFlags {
    let scope = query_param(url, "scope")
        .unwrap_or_else(|| "local".to_string())
        .to_ascii_lowercase();
    let mut include_local = matches!(scope.as_str(), "" | "local" | "all");
    let mut include_public = matches!(scope.as_str(), "public" | "remote" | "all");

    if query_param(url, "include_public").as_deref() == Some("true") {
        include_public = true;
    }
    if query_param(url, "include_local").as_deref() == Some("false") {
        include_local = false;
    }
    let confirm_public_sensitive = matches!(
        query_param(url, "confirm_public_sensitive").as_deref(),
        Some("true" | "1" | "yes" | "on")
    );

    OwnerSearchFlags {
        include_local,
        include_public,
        confirm_public_sensitive,
        public_options: OwnerPublicSearchOptions::from_url(url),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnerPublicSearchProvider {
    All,
    Bluesky,
    ActivityPub,
    Tootfinder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnerPublicSearchResultType {
    All,
    Posts,
    Actors,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OwnerPublicSearchFilters {
    sort: Option<String>,
    since: Option<String>,
    until: Option<String>,
    author: Option<String>,
    mentions: Option<String>,
    lang: Option<String>,
    domain: Option<String>,
    url: Option<String>,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerPublicSearchOptions {
    provider: OwnerPublicSearchProvider,
    result_type: OwnerPublicSearchResultType,
    activitypub_servers: Vec<String>,
    filters: OwnerPublicSearchFilters,
}

impl Default for OwnerPublicSearchOptions {
    fn default() -> Self {
        Self {
            provider: OwnerPublicSearchProvider::All,
            result_type: OwnerPublicSearchResultType::All,
            activitypub_servers: Vec::new(),
            filters: OwnerPublicSearchFilters::default(),
        }
    }
}

impl OwnerPublicSearchOptions {
    fn from_url(url: &worker::Url) -> Self {
        Self {
            provider: owner_public_search_provider(
                query_param(url, "provider").as_deref().unwrap_or("all"),
            ),
            result_type: owner_public_search_result_type(
                query_param(url, "type")
                    .or_else(|| query_param(url, "result_type"))
                    .as_deref()
                    .unwrap_or("all"),
            ),
            activitypub_servers: public_search_query_values(
                url,
                &[
                    "server",
                    "servers",
                    "activitypub_server",
                    "activitypub_servers",
                ],
            )
            .into_iter()
            .filter_map(|value| normalize_host_value(&value).ok())
            .take(MAX_ACTIVITYPUB_SEARCH_SERVERS)
            .collect(),
            filters: OwnerPublicSearchFilters {
                sort: public_search_sort(query_param(url, "sort")),
                since: non_empty_query_param(url, "since"),
                until: non_empty_query_param(url, "until"),
                author: non_empty_query_param(url, "author"),
                mentions: non_empty_query_param(url, "mentions"),
                lang: non_empty_query_param(url, "lang"),
                domain: non_empty_query_param(url, "domain"),
                url: non_empty_query_param(url, "url"),
                tags: public_search_query_values(url, &["tag", "tags"])
                    .into_iter()
                    .map(|value| value.trim().trim_start_matches('#').to_string())
                    .filter(|value| !value.is_empty())
                    .take(8)
                    .collect(),
            },
        }
    }

    fn includes_bluesky(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All | OwnerPublicSearchProvider::Bluesky
        )
    }

    fn includes_activitypub(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All | OwnerPublicSearchProvider::ActivityPub
        )
    }

    fn includes_tootfinder(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All
                | OwnerPublicSearchProvider::ActivityPub
                | OwnerPublicSearchProvider::Tootfinder
        )
    }

    fn includes_posts(&self) -> bool {
        matches!(
            self.result_type,
            OwnerPublicSearchResultType::All | OwnerPublicSearchResultType::Posts
        )
    }

    fn includes_actors(&self) -> bool {
        matches!(
            self.result_type,
            OwnerPublicSearchResultType::All | OwnerPublicSearchResultType::Actors
        )
    }
}

fn owner_public_search_provider(value: &str) -> OwnerPublicSearchProvider {
    match value.trim().to_ascii_lowercase().as_str() {
        "bluesky" | "bsky" | "atproto" | "at" => OwnerPublicSearchProvider::Bluesky,
        "activitypub" | "ap" | "mastodon" | "fediverse" => OwnerPublicSearchProvider::ActivityPub,
        "tootfinder" | "tootfinder.ch" | "activitypub-index" | "activitypub_index" | "index" => {
            OwnerPublicSearchProvider::Tootfinder
        }
        _ => OwnerPublicSearchProvider::All,
    }
}

fn owner_public_search_result_type(value: &str) -> OwnerPublicSearchResultType {
    match value.trim().to_ascii_lowercase().as_str() {
        "post" | "posts" | "status" | "statuses" => OwnerPublicSearchResultType::Posts,
        "actor" | "actors" | "account" | "accounts" | "profile" | "profiles" => {
            OwnerPublicSearchResultType::Actors
        }
        _ => OwnerPublicSearchResultType::All,
    }
}

fn public_search_sort(value: Option<String>) -> Option<String> {
    match value
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("top") => Some("top".to_string()),
        Some("latest") | Some("recent") | Some("new") => Some("latest".to_string()),
        _ => None,
    }
}

fn non_empty_query_param(url: &worker::Url, key: &str) -> Option<String> {
    query_param(url, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn public_search_query_values(url: &worker::Url, keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for (name, value) in url.query_pairs() {
        if !keys.iter().any(|key| name == *key) {
            continue;
        }
        for part in value.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                values.push(trimmed.to_string());
            }
        }
    }
    values
}

async fn owner_search(
    env: &Env,
    query: String,
    limit: i32,
    flags: OwnerSearchFlags,
) -> Result<OwnerSearch> {
    let term = query.trim().to_string();
    if term.is_empty() {
        return Ok(OwnerSearch {
            posts: Vec::new(),
            users: Vec::new(),
            sources: Vec::new(),
            source_items: Vec::new(),
            public_posts: Vec::new(),
            public_actors: Vec::new(),
            provider_errors: Vec::new(),
            public_search_guard: OwnerPublicSearchGuard::default(),
        });
    }

    let (posts, users, sources, source_items) = if flags.include_local {
        owner_local_search(env, &term, limit).await?
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };
    let public_categories = if flags.include_public {
        detect_sensitive_categories(&term)
    } else {
        Vec::new()
    };
    let public_guard =
        owner_public_search_guard(&public_categories, flags.confirm_public_sensitive);
    let public = if flags.include_public && !public_guard.blocked {
        owner_public_search(env, &term, limit, &flags.public_options).await
    } else {
        OwnerPublicSearch::default()
    };

    Ok(OwnerSearch {
        posts,
        users,
        sources,
        source_items,
        public_posts: public.posts,
        public_actors: public.actors,
        provider_errors: public.provider_errors,
        public_search_guard: public_guard,
    })
}

fn owner_public_search_guard(
    categories: &[String],
    confirm_public_sensitive: bool,
) -> OwnerPublicSearchGuard {
    let requires_confirmation = !categories.is_empty();
    let confirmed = requires_confirmation && confirm_public_sensitive;
    let blocked = requires_confirmation && !confirmed;
    let message = if blocked {
        Some("Public provider search skipped until the operator confirms this sensitive query.")
    } else if confirmed {
        Some("Sensitive public search was explicitly confirmed by the operator.")
    } else {
        None
    };
    OwnerPublicSearchGuard {
        blocked,
        requires_confirmation,
        confirmed,
        categories: categories.to_vec(),
        message: message.map(str::to_string),
    }
}

type OwnerLocalSearchRows = (
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
);

async fn owner_local_search(env: &Env, term: &str, limit: i32) -> Result<OwnerLocalSearchRows> {
    let db = env.d1("DB")?;
    let like = format!("%{term}%");
    let like_arg = D1Type::Text(&like);
    let limit_arg = D1Type::Integer(limit);
    let posts = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, start_time, end_time, location, poll_options,
                   visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, in_reply_to, atproto_uri, encrypted_message, media_attachments
            FROM posts
            WHERE content LIKE ?1 OR name LIKE ?1 OR summary LIKE ?1
            ORDER BY published_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let users = db
        .prepare(
            r#"
            SELECT follower_actor_id AS actor_id, 'follower' AS relation, status, created_at
            FROM followers
            WHERE follower_actor_id LIKE ?1
            UNION ALL
            SELECT target_actor_id AS actor_id, 'following' AS relation, status, created_at
            FROM following
            WHERE target_actor_id LIKE ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let sources = db
        .prepare(
            r#"
            SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
                   last_fetched_at, next_fetch_at, last_error, error_count, policy_json,
                   created_at, updated_at
            FROM source_subscriptions
            WHERE url LIKE ?1 OR title LIKE ?1 OR homepage_url LIKE ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let source_items = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, excerpt, published_at,
                   read, rights_policy_json, created_at
            FROM source_items
            WHERE title LIKE ?1 OR canonical_url LIKE ?1 OR excerpt LIKE ?1
            ORDER BY COALESCE(published_at, created_at) DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;

    Ok((posts, users, sources, source_items))
}

#[derive(Default)]
struct OwnerPublicSearch {
    posts: Vec<Map<String, Value>>,
    actors: Vec<Map<String, Value>>,
    provider_errors: Vec<Map<String, Value>>,
}

async fn owner_public_search(
    env: &Env,
    term: &str,
    limit: i32,
    options: &OwnerPublicSearchOptions,
) -> OwnerPublicSearch {
    let limit = limit.clamp(1, 25);
    let mut results = OwnerPublicSearch::default();

    if options.includes_bluesky() && options.includes_posts() {
        match owner_public_search_bluesky_posts(term, limit, &options.filters).await {
            Ok(posts) => results.posts.extend(posts),
            Err(error) => results
                .provider_errors
                .push(owner_search_provider_error("bluesky", "atproto", &error)),
        }
    }

    if options.includes_bluesky() && options.includes_actors() {
        match owner_public_search_bluesky_actors(term, limit).await {
            Ok(actors) => results.actors.extend(actors),
            Err(error) => results
                .provider_errors
                .push(owner_search_provider_error("bluesky", "atproto", &error)),
        }
    }

    if options.includes_activitypub() {
        if options.includes_posts() {
            if let Some(post) = owner_public_search_activitypub_direct_post(term).await {
                results.posts.push(post);
            }
        }
        if options.includes_actors() {
            if let Some(actor) = owner_public_search_activitypub_direct_actor(term).await {
                results.actors.push(actor);
            }
        }
        for server in activitypub_search_servers(env, options) {
            match owner_public_search_mastodon(&server, term, limit, options).await {
                Ok((posts, actors)) => {
                    results.posts.extend(posts);
                    results.actors.extend(actors);
                }
                Err(error) => results.provider_errors.push(owner_search_provider_error(
                    &server,
                    "activitypub",
                    &error,
                )),
            }
        }
    }
    if options.includes_tootfinder() && options.includes_posts() {
        match owner_public_search_tootfinder(term, limit).await {
            Ok(posts) => results.posts.extend(posts),
            Err(error) => results.provider_errors.push(owner_search_provider_error(
                "tootfinder.ch",
                "activitypub",
                &error,
            )),
        }
    }

    dedupe_owner_public_search(&mut results);
    results
}

async fn owner_public_search_bluesky_posts(
    term: &str,
    limit: i32,
    filters: &OwnerPublicSearchFilters,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let mut params = vec![
        ("q".to_string(), term.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if let Some(sort) = filters.sort.as_deref() {
        params.push(("sort".to_string(), sort.to_string()));
    }
    for (key, value) in [
        ("since", filters.since.as_deref()),
        ("until", filters.until.as_deref()),
        ("author", filters.author.as_deref()),
        ("mentions", filters.mentions.as_deref()),
        ("lang", filters.lang.as_deref()),
        ("domain", filters.domain.as_deref()),
        ("url", filters.url.as_deref()),
    ] {
        if let Some(value) = value {
            params.push((key.to_string(), value.to_string()));
        }
    }
    for tag in &filters.tags {
        params.push(("tag".to_string(), tag.to_string()));
    }
    let url = bluesky_appview_xrpc_url("app.bsky.feed.searchPosts", &encoded_query(&params));
    let body = fetch_json_with_accept(&url, "application/json", "bluesky post search").await?;
    Ok(body
        .get("posts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(owner_normalize_bluesky_post)
        .collect())
}

async fn owner_public_search_bluesky_actors(
    term: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let url = format!(
        "{}?q={}&limit={}",
        bluesky_appview_xrpc_url("app.bsky.actor.searchActors", ""),
        urlencoding::encode(term),
        limit
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky actor search").await?;
    Ok(body
        .get("actors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(owner_normalize_bluesky_actor)
        .collect())
}

async fn owner_public_search_mastodon(
    server: &str,
    term: &str,
    limit: i32,
    options: &OwnerPublicSearchOptions,
) -> std::result::Result<(Vec<Map<String, Value>>, Vec<Map<String, Value>>), String> {
    let params = owner_public_search_mastodon_query_params(term, limit, &options.result_type);
    let url = format!("https://{server}/api/v2/search?{}", encoded_query(&params));
    let body =
        fetch_json_with_accept(&url, "application/json", "mastodon-compatible search").await?;
    let posts = body
        .get("statuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|value| owner_normalize_mastodon_status(server, value))
        .collect();
    let actors = body
        .get("accounts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|value| owner_normalize_mastodon_account(server, value))
        .collect();
    Ok((posts, actors))
}

fn owner_public_search_mastodon_query_params(
    term: &str,
    limit: i32,
    result_type: &OwnerPublicSearchResultType,
) -> Vec<(String, String)> {
    let mut params = vec![
        ("q".to_string(), term.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    match result_type {
        OwnerPublicSearchResultType::Posts => {
            params.push(("type".to_string(), "statuses".to_string()));
        }
        OwnerPublicSearchResultType::Actors => {
            params.push(("type".to_string(), "accounts".to_string()));
        }
        OwnerPublicSearchResultType::All => {}
    }
    params
}

fn bluesky_appview_xrpc_url(method: &str, query: &str) -> String {
    let base = format!("{BLUESKY_APPVIEW_BASE_URL}/xrpc/{method}");
    if query.is_empty() {
        base
    } else {
        format!("{base}?{query}")
    }
}

fn activitypub_search_servers(env: &Env, options: &OwnerPublicSearchOptions) -> Vec<String> {
    let mut servers = Vec::new();
    if !options.activitypub_servers.is_empty() {
        servers.extend(options.activitypub_servers.iter().cloned());
    } else if let Ok(configured) = env.var("DAIS_ACTIVITYPUB_SEARCH_SERVERS") {
        for value in configured.to_string().split(',') {
            if let Ok(host) = normalize_host_value(value) {
                servers.push(host.to_string());
            }
        }
    }
    if servers.is_empty() {
        servers.extend(
            DEFAULT_ACTIVITYPUB_SEARCH_SERVERS
                .iter()
                .map(|value| (*value).to_string()),
        );
    }
    let mut seen = HashSet::new();
    servers
        .into_iter()
        .filter(|server| seen.insert(server.clone()))
        .take(MAX_ACTIVITYPUB_SEARCH_SERVERS)
        .collect()
}

async fn owner_public_search_tootfinder(
    term: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let url = tootfinder_search_url(term);
    let body =
        fetch_lenient_json_with_accept(&url, "application/json", "tootfinder search").await?;
    Ok(tootfinder_search_items(&body)
        .into_iter()
        .filter_map(owner_normalize_tootfinder_status)
        .take(limit.max(0) as usize)
        .collect())
}

fn tootfinder_search_url(term: &str) -> String {
    format!(
        "{TOOTFINDER_SEARCH_BASE_URL}/{}",
        urlencoding::encode(term.trim())
    )
}

fn tootfinder_search_items(body: &Value) -> Vec<Value> {
    if let Some(items) = body.as_array() {
        return items.clone();
    }
    body.get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn encoded_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

async fn owner_public_search_activitypub_direct_post(term: &str) -> Option<Map<String, Value>> {
    let discovered = discover_public_post_target(term).await?;
    Some(owner_public_post_row_from_discovered("direct", &discovered))
}

async fn owner_public_search_activitypub_direct_actor(term: &str) -> Option<Map<String, Value>> {
    let remote = resolve_activitypub_actor(term).await.ok()?;
    let handle = actor_handle(&remote);
    let summary = remote.summary.as_deref().map(strip_html);
    let mut row = owner_public_actor_row(
        "direct",
        "activitypub",
        &remote.id,
        handle.clone(),
        remote.name.clone(),
        summary,
        remote.url.clone(),
        remote.icon_url.clone(),
    );
    insert_optional_string(
        &mut row,
        "watch_type",
        Some("activitypub_actor".to_string()),
    );
    insert_optional_string(
        &mut row,
        "watch_target",
        remote.url.clone().or_else(|| Some(remote.id.clone())),
    );
    insert_optional_string(
        &mut row,
        "follow_target",
        remote.url.clone().or_else(|| Some(remote.id.clone())),
    );
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("follow".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

fn owner_public_post_row_from_discovered(
    provider: &str,
    discovered: &Map<String, Value>,
) -> Map<String, Value> {
    let id = string_field(Some(discovered), "id").unwrap_or_default();
    let url = string_field(Some(discovered), "url").unwrap_or_else(|| id.clone());
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: provider.to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id: string_field(Some(discovered), "actor_id"),
        actor_handle: None,
        actor_display_name: None,
        content: string_field(Some(discovered), "content")
            .or_else(|| string_field(Some(discovered), "summary"))
            .unwrap_or_default(),
        content_html: None,
        summary: string_field(Some(discovered), "summary"),
        object_type: string_field(Some(discovered), "type"),
        published_at: string_field(Some(discovered), "published"),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    row
}

fn dedupe_owner_public_search(results: &mut OwnerPublicSearch) {
    let mut seen_posts = HashSet::new();
    results.posts.retain(|row| {
        let key = string_field(Some(row), "id")
            .or_else(|| string_field(Some(row), "url"))
            .unwrap_or_default();
        !key.is_empty() && seen_posts.insert(key)
    });
    let mut seen_actors = HashSet::new();
    results.actors.retain(|row| {
        let key = string_field(Some(row), "id")
            .or_else(|| string_field(Some(row), "url"))
            .or_else(|| string_field(Some(row), "handle"))
            .unwrap_or_default();
        !key.is_empty() && seen_actors.insert(key)
    });
}

fn owner_normalize_bluesky_post(value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let uri = object.get("uri").and_then(optional_body_string)?;
    let author = object.get("author").and_then(Value::as_object);
    let handle = author
        .and_then(|row| row.get("handle"))
        .and_then(optional_body_string);
    let actor_id = author
        .and_then(|row| row.get("did"))
        .and_then(optional_body_string);
    let display_name = author
        .and_then(|row| row.get("displayName"))
        .and_then(optional_body_string);
    let record = object.get("record").and_then(Value::as_object);
    let text = record
        .and_then(|row| row.get("text"))
        .and_then(optional_body_string)
        .unwrap_or_default();
    let published_at = record
        .and_then(|row| row.get("createdAt"))
        .and_then(optional_body_string)
        .or_else(|| object.get("indexedAt").and_then(optional_body_string));
    let url = bluesky_post_url(&uri, handle.as_deref()).unwrap_or_else(|| uri.clone());
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: "bluesky".to_string(),
        network: "atproto".to_string(),
        id: uri.clone(),
        url: url.clone(),
        actor_id,
        actor_handle: handle,
        actor_display_name: display_name,
        content: text,
        content_html: None,
        summary: None,
        object_type: Some("app.bsky.feed.post".to_string()),
        published_at,
    });
    insert_optional_string(
        &mut row,
        "cid",
        object.get("cid").and_then(optional_body_string),
    );
    insert_optional_number(&mut row, "reply_count", object.get("replyCount"));
    insert_optional_number(&mut row, "repost_count", object.get("repostCount"));
    insert_optional_number(&mut row, "like_count", object.get("likeCount"));
    owner_add_public_post_actions(&mut row, "atproto", Some("bluesky_post"), &uri, &url);
    Some(row)
}

fn owner_normalize_bluesky_actor(value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let did = object.get("did").and_then(optional_body_string)?;
    let handle = object.get("handle").and_then(optional_body_string);
    let url = handle
        .as_ref()
        .map(|handle| format!("https://bsky.app/profile/{handle}"))
        .unwrap_or_else(|| did.clone());
    let mut row = owner_public_actor_row(
        "bluesky",
        "atproto",
        &did,
        handle.clone(),
        object.get("displayName").and_then(optional_body_string),
        object.get("description").and_then(optional_body_string),
        Some(url),
        object.get("avatar").and_then(optional_body_string),
    );
    insert_optional_string(&mut row, "watch_type", Some("bluesky_actor".to_string()));
    insert_optional_string(&mut row, "watch_target", handle.or_else(|| Some(did)));
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

fn owner_normalize_mastodon_status(provider: &str, value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("uri")
        .or_else(|| object.get("url"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let url = object
        .get("url")
        .and_then(optional_body_string)
        .unwrap_or_else(|| id.clone());
    let content_html = object
        .get("content")
        .and_then(optional_body_string)
        .unwrap_or_default();
    let summary = object.get("spoiler_text").and_then(optional_body_string);
    let mut content = strip_html(&content_html);
    if content.is_empty() {
        content = summary.clone().unwrap_or_default();
    }
    let account = object.get("account").and_then(Value::as_object);
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: provider.to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id: account
            .and_then(|row| row.get("url"))
            .and_then(optional_body_string),
        actor_handle: account
            .and_then(|row| row.get("acct"))
            .and_then(optional_body_string)
            .or_else(|| {
                account
                    .and_then(|row| row.get("username"))
                    .and_then(optional_body_string)
            }),
        actor_display_name: account
            .and_then(|row| row.get("display_name"))
            .and_then(optional_body_string),
        content,
        content_html: (!content_html.is_empty()).then_some(content_html),
        summary,
        object_type: Some("Note".to_string()),
        published_at: object.get("created_at").and_then(optional_body_string),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    Some(row)
}

fn owner_normalize_mastodon_account(provider: &str, value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("url")
        .or_else(|| object.get("uri"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let actor_url = object.get("url").and_then(optional_body_string);
    let mut row = owner_public_actor_row(
        provider,
        "activitypub",
        &id,
        object
            .get("acct")
            .and_then(optional_body_string)
            .or_else(|| object.get("username").and_then(optional_body_string)),
        object.get("display_name").and_then(optional_body_string),
        object
            .get("note")
            .and_then(optional_body_string)
            .map(|html| strip_html(&html)),
        actor_url.clone(),
        object.get("avatar").and_then(optional_body_string),
    );
    insert_optional_string(
        &mut row,
        "watch_type",
        Some("activitypub_actor".to_string()),
    );
    insert_optional_string(
        &mut row,
        "watch_target",
        actor_url.clone().or_else(|| Some(id.clone())),
    );
    insert_optional_string(&mut row, "follow_target", actor_url.or_else(|| Some(id)));
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("follow".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

fn owner_normalize_tootfinder_status(value: Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("uri")
        .or_else(|| object.get("url"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let url = object
        .get("url")
        .and_then(optional_body_string)
        .unwrap_or_else(|| id.clone());
    let content_html = object
        .get("content")
        .and_then(optional_body_string)
        .unwrap_or_default();
    let summary = object
        .get("spoiler_text")
        .or_else(|| object.get("spoiler"))
        .and_then(optional_body_string);
    let mut content = strip_html(&content_html);
    if content.is_empty() {
        content = summary.clone().unwrap_or_default();
    }
    let actor_id = activitypub_actor_id_from_status_uri(&id).or_else(|| {
        object
            .get("uri")
            .and_then(optional_body_string)
            .and_then(|uri| activitypub_actor_id_from_status_uri(&uri))
    });
    let actor_handle = actor_handle_from_public_status_url(&url)
        .or_else(|| actor_id.as_deref().and_then(actor_handle_from_actor_url));
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: "tootfinder.ch".to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id,
        actor_handle,
        actor_display_name: None,
        content,
        content_html: (!content_html.is_empty()).then_some(content_html),
        summary,
        object_type: Some("Note".to_string()),
        published_at: object.get("created_at").and_then(optional_body_string),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    insert_optional_string(
        &mut row,
        "language",
        object.get("language").and_then(optional_body_string),
    );
    Some(row)
}

fn activitypub_actor_id_from_status_uri(uri: &str) -> Option<String> {
    let trimmed = uri.trim();
    let (actor, _) = trimmed.split_once("/statuses/")?;
    (!actor.is_empty()).then(|| actor.to_string())
}

fn actor_handle_from_public_status_url(url: &str) -> Option<String> {
    let parsed = worker::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();
    let username = path
        .strip_prefix("/@")
        .and_then(|rest| rest.split('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!("{username}@{host}"))
}

fn actor_handle_from_actor_url(url: &str) -> Option<String> {
    let parsed = worker::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();
    let username = path
        .strip_prefix("/users/")
        .or_else(|| path.strip_prefix("/@"))
        .and_then(|rest| rest.split('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!("{username}@{host}"))
}

struct OwnerPublicPostFields {
    provider: String,
    network: String,
    id: String,
    url: String,
    actor_id: Option<String>,
    actor_handle: Option<String>,
    actor_display_name: Option<String>,
    content: String,
    content_html: Option<String>,
    summary: Option<String>,
    object_type: Option<String>,
    published_at: Option<String>,
}

fn owner_public_post_row(fields: OwnerPublicPostFields) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(fields.provider));
    row.insert("network".to_string(), Value::String(fields.network));
    row.insert("id".to_string(), Value::String(fields.id));
    row.insert("url".to_string(), Value::String(fields.url));
    row.insert("content".to_string(), Value::String(fields.content));
    insert_optional_string(&mut row, "actor_id", fields.actor_id);
    insert_optional_string(&mut row, "actor_handle", fields.actor_handle);
    insert_optional_string(&mut row, "actor_display_name", fields.actor_display_name);
    insert_optional_string(&mut row, "content_html", fields.content_html);
    insert_optional_string(&mut row, "summary", fields.summary);
    insert_optional_string(&mut row, "object_type", fields.object_type);
    insert_optional_string(&mut row, "published_at", fields.published_at);
    row
}

fn owner_add_public_post_actions(
    row: &mut Map<String, Value>,
    network: &str,
    watch_type: Option<&str>,
    id: &str,
    url: &str,
) {
    if let Some(watch_type) = watch_type {
        insert_optional_string(row, "watch_type", Some(watch_type.to_string()));
        insert_optional_string(row, "watch_target", Some(id.to_string()));
    }
    insert_optional_string(row, "reply_target", Some(id.to_string()));
    let mut actions = vec![
        Value::String("open".to_string()),
        Value::String("watch".to_string()),
        Value::String("reply".to_string()),
    ];
    if network == "activitypub" {
        actions.push(Value::String("like".to_string()));
        actions.push(Value::String("boost".to_string()));
    }
    row.insert("actions".to_string(), Value::Array(actions));
    if !url.is_empty() {
        insert_optional_string(row, "canonical_url", Some(url.to_string()));
    }
}

fn owner_public_actor_row(
    provider: &str,
    network: &str,
    id: &str,
    handle: Option<String>,
    display_name: Option<String>,
    summary: Option<String>,
    url: Option<String>,
    avatar_url: Option<String>,
) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(provider.to_string()));
    row.insert("network".to_string(), Value::String(network.to_string()));
    row.insert("id".to_string(), Value::String(id.to_string()));
    insert_optional_string(&mut row, "handle", handle);
    insert_optional_string(&mut row, "display_name", display_name);
    insert_optional_string(&mut row, "summary", summary);
    insert_optional_string(&mut row, "url", url);
    insert_optional_string(&mut row, "avatar_url", avatar_url);
    row
}

fn owner_search_provider_error(provider: &str, network: &str, error: &str) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(provider.to_string()));
    row.insert("network".to_string(), Value::String(network.to_string()));
    row.insert("error".to_string(), Value::String(error.to_string()));
    row
}

fn bluesky_post_url(uri: &str, handle: Option<&str>) -> Option<String> {
    let handle = handle?.trim();
    let rkey = uri.rsplit('/').next()?.trim();
    if handle.is_empty() || rkey.is_empty() {
        return None;
    }
    Some(format!("https://bsky.app/profile/{handle}/post/{rkey}"))
}

fn insert_optional_string(row: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        row.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_number(row: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(Value::Number(number)) = value {
        row.insert(key.to_string(), Value::Number(number.clone()));
    }
}

async fn owner_source_subscriptions(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
        WHERE source_type NOT IN (
          'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
          'watch_bluesky_actor', 'watch_bluesky_post'
        )
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_watch_subscriptions(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
        WHERE source_type IN (
          'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
          'watch_bluesky_actor', 'watch_bluesky_post'
        )
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_source_items(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, thumbnail_url,
                   rights_policy_json, read, summary, created_at, updated_at
            FROM source_items
            WHERE source_type NOT IN (
              'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
              'watch_bluesky_actor', 'watch_bluesky_post'
            )
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(normalize_source_item).collect())
}

async fn owner_watch_items(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, thumbnail_url,
                   rights_policy_json, read, summary, created_at, updated_at
            FROM source_items
            WHERE source_type IN (
              'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
              'watch_bluesky_actor', 'watch_bluesky_post'
            )
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(normalize_source_item).collect())
}

async fn owner_add_source(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let source_type = normalize_source_type(
        &string_like_any(body, &["source_type", "sourceType"]).unwrap_or_default(),
    );
    if !is_addable_source_type(&source_type) {
        return Err(format!(
            "source_type must be one of: {}",
            addable_source_types().join(", ")
        ));
    }
    let source_url = normalized_source_target(&source_type, body)?;
    let title = body.get("title").and_then(optional_body_string);
    let cadence_minutes = clamp_cadence_minutes(string_like_any(
        body,
        &["cadence_minutes", "cadenceMinutes"],
    ));
    let api_secret_name = if is_watch_source_type(&source_type) {
        None
    } else {
        string_like_any(body, &["api_secret_name", "apiSecretName"])
            .and_then(|value| optional_body_string(&Value::String(value)))
    };
    let policy_json = source_policy_json_for_type(body, &source_type);

    owner_upsert_source(
        env,
        &source_type,
        &source_url,
        title.as_deref(),
        cadence_minutes,
        api_secret_name.as_deref(),
        &policy_json,
    )
    .await
}

async fn owner_add_watch(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let watch_kind = string_like_any(
        body,
        &[
            "watch_type",
            "watchType",
            "source_type",
            "sourceType",
            "protocol",
            "kind",
        ],
    )
    .unwrap_or_else(|| "rss".to_string());
    let source_type = source_type_for_watch_kind(&watch_kind)
        .ok_or_else(|| "watch_type must be rss, atom, activitypub_actor, activitypub_object, bluesky_actor, or bluesky_post".to_string())?;
    let source_url = normalized_source_target(source_type, body)?;
    let id = source_id(source_type, &source_url);
    let title = body.get("title").and_then(optional_body_string);
    let cadence_minutes = clamp_cadence_minutes(string_like_any(
        body,
        &["cadence_minutes", "cadenceMinutes"],
    ));
    let policy_json = source_policy_json_for_type(body, source_type);

    owner_upsert_source(
        env,
        source_type,
        &source_url,
        title.as_deref(),
        cadence_minutes,
        None,
        &policy_json,
    )
    .await
    .map(|mut row| {
        row.insert("watch".to_string(), Value::Bool(true));
        row.insert(
            "watch_type".to_string(),
            Value::String(source_type.to_string()),
        );
        row.insert("id".to_string(), Value::String(id));
        row
    })
}

async fn owner_upsert_source(
    env: &Env,
    source_type: &str,
    source_url: &str,
    title: Option<&str>,
    cadence_minutes: i32,
    api_secret_name: Option<&str>,
    policy_json: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let id = source_id(source_type, source_url);
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&id);
    let type_arg = D1Type::Text(source_type);
    let url_arg = D1Type::Text(source_url);
    let title_arg = title.map(D1Type::Text).unwrap_or(D1Type::Null);
    let cadence_arg = D1Type::Integer(cadence_minutes);
    let policy_arg = D1Type::Text(policy_json);
    let secret_arg = api_secret_name.map(D1Type::Text).unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        INSERT INTO source_subscriptions (
          id, source_type, url, title, refresh_cadence_minutes, policy_json,
          api_secret_name, status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          source_type = excluded.source_type,
          url = excluded.url,
          title = excluded.title,
          refresh_cadence_minutes = excluded.refresh_cadence_minutes,
          policy_json = excluded.policy_json,
          api_secret_name = excluded.api_secret_name,
          status = 'active',
          last_error = NULL,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([
        &id_arg,
        &type_arg,
        &url_arg,
        &title_arg,
        &cadence_arg,
        &policy_arg,
        &secret_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    owner_source_by_id(env, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "source add failed".to_string())
}

async fn owner_source_by_id(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE id = ?1
        "#,
    )
    .bind_refs(&id_arg)?
    .first::<Map<String, Value>>(None)
    .await
}

async fn owner_refresh_sources(env: &Env, id: Option<&str>) -> std::result::Result<Value, String> {
    let rows = if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
        match owner_source_by_id(env, id)
            .await
            .map_err(|error| error.to_string())?
        {
            Some(source) => vec![source],
            None => return Err(format!("source not found: {id}")),
        }
    } else {
        owner_active_sources(env)
            .await
            .map_err(|error| error.to_string())?
    };
    refresh_source_rows(env, rows).await
}

async fn owner_refresh_watches(env: &Env, id: Option<&str>) -> std::result::Result<Value, String> {
    let rows = if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
        match owner_source_by_id(env, id)
            .await
            .map_err(|error| error.to_string())?
        {
            Some(source)
                if string_field(Some(&source), "source_type")
                    .map(|source_type| is_watch_source_type(&source_type))
                    .unwrap_or(false) =>
            {
                vec![source]
            }
            Some(_) => return Err(format!("source is not a watch: {id}")),
            None => return Err(format!("watch not found: {id}")),
        }
    } else {
        owner_active_watches(env)
            .await
            .map_err(|error| error.to_string())?
    };
    refresh_source_rows(env, rows).await
}

async fn refresh_source_rows(
    env: &Env,
    rows: Vec<Map<String, Value>>,
) -> std::result::Result<Value, String> {
    let mut items = Vec::new();
    for source in rows {
        let source_id = string_field(Some(&source), "id").unwrap_or_default();
        match refresh_feed_source(env, &source).await {
            Ok(()) => {
                let status = owner_source_by_id(env, &source_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|row| string_field(Some(&row), "status"))
                    .unwrap_or_else(|| "active".to_string());
                items.push(serde_json::json!({ "id": source_id, "ok": true, "status": status }));
            }
            Err(message) => {
                let message = truncate_chars(&message, 500);
                mark_source_error(env, &source_id, &message).await?;
                items.push(serde_json::json!({ "id": source_id, "ok": false, "error": message }));
            }
        }
    }
    let ok = items
        .iter()
        .all(|item| item.get("ok").and_then(Value::as_bool).unwrap_or(false));
    Ok(serde_json::json!({ "ok": ok, "items": items }))
}

async fn refresh_due_sources(env: &Env) -> std::result::Result<(), String> {
    let rows = due_active_sources(env)
        .await
        .map_err(|error| error.to_string())?;
    for source in rows {
        if let Err(message) = refresh_feed_source(env, &source).await {
            if let Some(source_id) = string_field(Some(&source), "id") {
                let message = truncate_chars(&message, 500);
                mark_source_error(env, &source_id, &message).await?;
            }
        }
    }
    Ok(())
}

async fn owner_active_sources(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'rss', 'atom', 'api', 'watch_rss', 'watch_atom',
            'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_active_watches(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn due_active_sources(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let now_arg = D1Type::Text(&now);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'rss', 'atom', 'api', 'watch_rss', 'watch_atom',
            'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
          AND (next_fetch_at IS NULL OR next_fetch_at <= ?1)
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .bind_refs(&now_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn refresh_feed_source(
    env: &Env,
    source: &Map<String, Value>,
) -> std::result::Result<(), String> {
    let source_id =
        string_field(Some(source), "id").ok_or_else(|| "source id is missing".to_string())?;
    let source_type =
        string_field(Some(source), "source_type").unwrap_or_else(|| "rss".to_string());
    if !is_refreshable_source_type(&source_type) {
        return Err(format!("unsupported source type {source_type}"));
    }
    let url =
        string_field(Some(source), "url").ok_or_else(|| "source url is missing".to_string())?;
    let cadence = row_int(source, "refresh_cadence_minutes")
        .unwrap_or(60)
        .max(5);
    let next_fetch_at = js_sys::Date::new(&JsValue::from_f64(
        js_sys::Date::now() + (cadence as f64) * 60.0 * 1000.0,
    ))
    .to_iso_string()
    .as_string()
    .unwrap_or_default();
    let policy = source_policy_from_row(source);

    if source_type == "watch_activitypub_actor" {
        let items = watch_activitypub_actor_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_activitypub_object" {
        let items = watch_activitypub_object_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_bluesky_actor" {
        let items = watch_bluesky_actor_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_bluesky_post" {
        let items = watch_bluesky_post_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }

    let mut response = fetch_source(env, source, &url).await?;
    let status = response.status_code();
    if status == 304 {
        mark_source_refreshed(
            env,
            &source_id,
            &next_fetch_at,
            string_field(Some(source), "etag").as_deref(),
            string_field(Some(source), "last_modified").as_deref(),
        )
        .await?;
        return Ok(());
    }
    if !(200..=299).contains(&status) {
        return Err(format!("source fetch failed with HTTP {status}"));
    }

    let etag = response
        .headers()
        .get("ETag")
        .map_err(|error| error.to_string())?
        .or_else(|| string_field(Some(source), "etag"));
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .map_err(|error| error.to_string())?
        .or_else(|| string_field(Some(source), "last_modified"));
    let body = response.text().await.map_err(|error| error.to_string())?;
    let mut items = if source_type == "api" {
        parse_api_items(&body, source, &policy)?
    } else {
        parse_feed_items(&body, source, &policy)
    };
    items.truncate(50);
    store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
    mark_source_refreshed(
        env,
        &source_id,
        &next_fetch_at,
        etag.as_deref(),
        last_modified.as_deref(),
    )
    .await?;
    Ok(())
}

async fn store_source_refresh_items(
    env: &Env,
    source_id: &str,
    source_type: &str,
    policy: &SourcePolicy,
    mut items: Vec<SourceRefreshItem>,
) -> std::result::Result<(), String> {
    items.truncate(50);
    for item in items {
        insert_source_item(env, source_id, source_type, policy, &item).await?;
    }
    Ok(())
}

async fn fetch_source(
    env: &Env,
    env_source: &Map<String, Value>,
    url: &str,
) -> std::result::Result<worker::Response, String> {
    let headers = Headers::new();
    headers
        .set("User-Agent", "dais-source-refresh/1.0")
        .map_err(|error| error.to_string())?;
    if let Some(etag) = string_field(Some(env_source), "etag") {
        headers
            .set("If-None-Match", &etag)
            .map_err(|error| error.to_string())?;
    }
    if let Some(last_modified) = string_field(Some(env_source), "last_modified") {
        headers
            .set("If-Modified-Since", &last_modified)
            .map_err(|error| error.to_string())?;
    }
    let source_type = string_field(Some(env_source), "source_type").unwrap_or_default();
    if !is_watch_source_type(&source_type) {
        if let Some(secret_name) = string_field(Some(env_source), "api_secret_name") {
            if let Ok(secret) = env.var(&secret_name) {
                headers
                    .set("Authorization", &format!("Bearer {}", secret.to_string()))
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())
}

async fn insert_source_item(
    env: &Env,
    source_id: &str,
    source_type: &str,
    policy: &SourcePolicy,
    item: &SourceRefreshItem,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let policy_json =
        serde_json::to_string(&policy.to_value()).map_err(|error| error.to_string())?;
    let metadata_json = serde_json::json!({ "scheduled": true }).to_string();
    let item_id_arg = D1Type::Text(&item.id);
    let source_id_arg = D1Type::Text(source_id);
    let source_type_arg = D1Type::Text(source_type);
    let title_arg = D1Type::Text(&item.title);
    let canonical_arg = item
        .canonical_url
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let external_arg = item
        .external_id
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let author_arg = item
        .author
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let published_arg = item
        .published_at
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let excerpt_arg = item
        .excerpt
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let content_type_arg = D1Type::Text("text/html");
    let hash_arg = D1Type::Text(&item.hash);
    let thumbnail_arg = item
        .thumbnail_url
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let policy_arg = D1Type::Text(&policy_json);
    let metadata_arg = D1Type::Text(&metadata_json);
    db.prepare(
        r#"
        INSERT OR IGNORE INTO source_items (
          id, source_id, source_type, title, canonical_url, external_id, author,
          published_at, excerpt, content_type, hash, thumbnail_url, rights_policy_json,
          raw_metadata_json, fetched_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind_refs([
        &item_id_arg,
        &source_id_arg,
        &source_type_arg,
        &title_arg,
        &canonical_arg,
        &external_arg,
        &author_arg,
        &published_arg,
        &excerpt_arg,
        &content_type_arg,
        &hash_arg,
        &thumbnail_arg,
        &policy_arg,
        &metadata_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn mark_source_refreshed(
    env: &Env,
    source_id: &str,
    next_fetch_at: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let next_arg = D1Type::Text(next_fetch_at);
    let etag_arg = etag.map(D1Type::Text).unwrap_or(D1Type::Null);
    let modified_arg = last_modified.map(D1Type::Text).unwrap_or(D1Type::Null);
    let id_arg = D1Type::Text(source_id);
    db.prepare(
        r#"
        UPDATE source_subscriptions
        SET status = 'active',
            last_fetched_at = CURRENT_TIMESTAMP,
            next_fetch_at = ?1,
            etag = COALESCE(?2, etag),
            last_modified = COALESCE(?3, last_modified),
            last_error = NULL,
            error_count = 0,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?4
        "#,
    )
    .bind_refs([&next_arg, &etag_arg, &modified_arg, &id_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn mark_source_error(
    env: &Env,
    source_id: &str,
    message: &str,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let message_arg = D1Type::Text(message);
    let id_arg = D1Type::Text(source_id);
    db.prepare(
        r#"
        UPDATE source_subscriptions
        SET status = 'error',
            last_error = ?1,
            error_count = error_count + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?2
        "#,
    )
    .bind_refs([&message_arg, &id_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn owner_delete_source(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("DELETE FROM source_subscriptions WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_delete_watch(env: &Env, id: &str) -> std::result::Result<(), String> {
    let Some(source) = owner_source_by_id(env, id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Err(format!("watch not found: {id}"));
    };
    let source_type = string_field(Some(&source), "source_type").unwrap_or_default();
    if !is_watch_source_type(&source_type) {
        return Err(format!("source is not a watch: {id}"));
    }
    owner_delete_source(env, id)
        .await
        .map_err(|error| error.to_string())
}

async fn owner_unblock(env: &Env, value: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let value_arg = D1Type::Text(value);
    db.prepare("DELETE FROM blocks WHERE id = ?1 OR actor_id = ?1 OR blocked_domain = ?1")
        .bind_refs(&value_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_block(env: &Env, body: &Value) -> std::result::Result<Map<String, Value>, String> {
    let reason = body.get("reason").and_then(optional_body_string);
    let actor_id = body_string_any(body, &["actor_id", "actorId"]);
    let domain = body_string_any(body, &["domain", "blocked_domain", "blockedDomain"]);

    if let Some(actor_id) = actor_id {
        let actor_url = public_https_url(&actor_id, "actor_id")?;
        let id = format!("block-{}", stable_id(&actor_url));
        insert_block(env, &id, &actor_url, None, reason.as_deref()).await?;

        let mut response = Map::new();
        response.insert("ok".to_string(), Value::Bool(true));
        response.insert("id".to_string(), Value::String(id));
        response.insert("actor_id".to_string(), Value::String(actor_url));
        return Ok(response);
    }

    if let Some(domain) = domain {
        let host = normalize_host_value(&domain).map_err(ToOwned::to_owned)?;
        let id = format!("block-domain-{host}");
        insert_block(env, &id, &host, Some(&host), reason.as_deref()).await?;

        let mut response = Map::new();
        response.insert("ok".to_string(), Value::Bool(true));
        response.insert("id".to_string(), Value::String(id));
        response.insert("blocked_domain".to_string(), Value::String(host));
        return Ok(response);
    }

    Err("actor_id or domain is required".to_string())
}

async fn insert_block(
    env: &Env,
    id: &str,
    actor_id: &str,
    blocked_domain: Option<&str>,
    reason: Option<&str>,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(id);
    let actor_arg = D1Type::Text(actor_id);
    let domain_arg = blocked_domain.map(D1Type::Text).unwrap_or(D1Type::Null);
    let reason_arg = reason.map(D1Type::Text).unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        INSERT OR REPLACE INTO blocks (id, actor_id, blocked_domain, reason, created_at)
        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        "#,
    )
    .bind_refs([&id_arg, &actor_arg, &domain_arg, &reason_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn owner_delete_allowlist_host(env: &Env, host: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let host_arg = D1Type::Text(host);
    db.prepare("DELETE FROM federation_allowlist WHERE host = ?1")
        .bind_refs(&host_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_allow_host(env: &Env, host: &str, note: Option<&str>) -> Result<Map<String, Value>> {
    let db = env.d1("DB")?;
    let host_arg = D1Type::Text(host);
    let note_arg = note.map(D1Type::Text).unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        INSERT INTO federation_allowlist (host, note, enabled, created_at, updated_at)
        VALUES (?1, ?2, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(host) DO UPDATE SET
          note = excluded.note,
          enabled = 1,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([&host_arg, &note_arg])?
    .run()
    .await?;

    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("host".to_string(), Value::String(host.to_string()));
    Ok(response)
}

async fn insert_delivery_rows(
    env: &Env,
    post_id: &str,
    inboxes: Vec<String>,
    activity_type: &str,
    activity_json: Option<String>,
) -> std::result::Result<Vec<String>, String> {
    let mut allowed_inboxes = Vec::new();
    for inbox in inboxes {
        if owner_federation_target_allowed(env, &inbox).await? {
            allowed_inboxes.push(inbox);
        }
    }
    let mut unique_inboxes = Vec::new();
    for inbox in allowed_inboxes
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        if !unique_inboxes.contains(&inbox) {
            unique_inboxes.push(inbox);
        }
    }

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let created_at = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let activity_json_arg = activity_json
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let activity_type_arg = D1Type::Text(activity_type);
    let post_arg = D1Type::Text(post_id);
    let created_arg = D1Type::Text(&created_at);
    let mut delivery_ids = Vec::new();

    for inbox in unique_inboxes {
        let delivery_id = format!(
            "delivery-{}",
            stable_id(&format!("{post_id}\n{inbox}\n{created_at}"))
                .chars()
                .take(24)
                .collect::<String>()
        );
        let delivery_arg = D1Type::Text(&delivery_id);
        let inbox_arg = D1Type::Text(&inbox);
        db.prepare(
            r#"
            INSERT INTO deliveries (
              id, post_id, target_type, target_url, protocol,
              status, retry_count, created_at, activity_type, activity_json
            ) VALUES (
              ?1, ?2, 'inbox', ?3, 'activitypub',
              'queued', 0, ?4, ?5, ?6
            )
            "#,
        )
        .bind_refs([
            &delivery_arg,
            &post_arg,
            &inbox_arg,
            &created_arg,
            &activity_type_arg,
            &activity_json_arg,
        ])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
        delivery_ids.push(delivery_id);
    }
    Ok(delivery_ids)
}

async fn owner_publish_interaction(
    env: &Env,
    object_id: &str,
    interaction: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if object_id.is_empty() {
        return Err("object_id is required".to_string());
    }
    let requested_object_id = public_https_url(object_id, "object_id")?;
    let object_id = canonical_mastodon_status_id(&requested_object_id);
    let undo = matches!(interaction, "unlike" | "unboost");
    let normalized = match interaction {
        "unlike" => "like",
        "unboost" => "boost",
        "like" | "boost" => interaction,
        _ => return Err("interaction must be like, unlike, boost, or unboost".to_string()),
    };
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let target_inbox = resolve_activitypub_object_inbox(&requested_object_id).await?;
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let activity_type = if normalized == "like" {
        "Like"
    } else {
        "Announce"
    };
    let activity_id = format!(
        "{}#{}s/{}",
        local_actor.id,
        normalized,
        stable_id(&object_id).chars().take(16).collect::<String>()
    );
    let outgoing_id = if undo {
        format!(
            "{}#undos/{}/{}",
            local_actor.id,
            normalized,
            stable_id(&format!("{object_id}\n{now}"))
                .chars()
                .take(16)
                .collect::<String>()
        )
    } else {
        activity_id.clone()
    };
    let activity = if undo {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": outgoing_id,
            "type": "Undo",
            "actor": local_actor.id,
            "published": now,
            "to": [PUBLIC_COLLECTION],
            "cc": [format!("{}/followers", local_actor.id)],
            "object": {
                "id": activity_id,
                "type": activity_type,
                "actor": local_actor.id,
                "object": requested_object_id,
            },
        })
    } else {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": outgoing_id,
            "type": activity_type,
            "actor": local_actor.id,
            "published": now,
            "to": [PUBLIC_COLLECTION],
            "cc": [format!("{}/followers", local_actor.id)],
            "object": requested_object_id,
        })
    };
    let delivery_ids = insert_delivery_rows(
        env,
        &object_id,
        vec![target_inbox],
        if undo { "Undo" } else { activity_type },
        Some(activity.to_string()),
    )
    .await?;

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    if undo {
        let activity_id_arg = D1Type::Text(&activity_id);
        db.prepare("DELETE FROM interactions WHERE id = ?1")
            .bind_refs(&activity_id_arg)
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
    } else {
        let activity_id_arg = D1Type::Text(&activity_id);
        let normalized_arg = D1Type::Text(normalized);
        let actor_arg = D1Type::Text(&local_actor.id);
        let object_arg = D1Type::Text(&object_id);
        let now_arg = D1Type::Text(&now);
        db.prepare(
            r#"
            INSERT OR REPLACE INTO interactions (
              id, type, actor_id, object_url, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind_refs([
            &activity_id_arg,
            &normalized_arg,
            &actor_arg,
            &object_arg,
            &now_arg,
        ])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    }

    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("activity_id".to_string(), Value::String(outgoing_id));
    response.insert(
        "interaction".to_string(),
        Value::String(if undo {
            format!("undo-{normalized}")
        } else {
            normalized.to_string()
        }),
    );
    response.insert("object_id".to_string(), Value::String(object_id));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(response)
}

async fn owner_follow_actor(
    env: &Env,
    target: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if target.is_empty() {
        return Err("target is required".to_string());
    }
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let remote = resolve_activitypub_actor_for_local(target, &local_actor).await?;
    if remote.id.is_empty() || remote.inbox.is_empty() {
        return Err("target actor must expose id and inbox".to_string());
    }
    if remote.id == local_actor.id {
        return Err("cannot follow the local actor".to_string());
    }
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let follow_id = format!(
        "{}#follows/{}",
        local_actor.id,
        stable_id(&format!("{}\n{now}", remote.id))
            .chars()
            .take(16)
            .collect::<String>()
    );
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": follow_id,
        "actor": local_actor.id,
        "object": remote.id,
        "to": [remote.id],
        "published": now,
    });

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let follow_arg = D1Type::Text(&follow_id);
    let local_arg = D1Type::Text(&local_actor.id);
    let remote_arg = D1Type::Text(&remote.id);
    let inbox_arg = D1Type::Text(&remote.inbox);
    let now_arg = D1Type::Text(&now);
    db.prepare(
        r#"
        INSERT INTO following (
          id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
        ) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL)
        ON CONFLICT(actor_id, target_actor_id) DO UPDATE SET
          id = excluded.id,
          target_inbox = excluded.target_inbox,
          status = 'pending',
          created_at = excluded.created_at,
          accepted_at = NULL
        "#,
    )
    .bind_refs([&follow_arg, &local_arg, &remote_arg, &inbox_arg, &now_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let delivery_ids = insert_delivery_rows(
        env,
        &follow_id,
        vec![remote.inbox.clone()],
        "Follow",
        Some(activity.to_string()),
    )
    .await?;
    let following = owner_following_row(env, &local_actor.id, &remote.id)
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("following".to_string(), Value::Object(following));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(response)
}

async fn owner_discover_actor(
    env: &Env,
    target: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if target.is_empty() {
        return Err("target is required".to_string());
    }
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let target_public_post = discover_public_post_target(target).await;
    let actor_target = target_public_post
        .as_ref()
        .and_then(|post| string_field(Some(post), "actor_id"))
        .unwrap_or_else(|| target.to_string());
    let remote = resolve_activitypub_actor_for_local(&actor_target, &local_actor).await?;
    if remote.inbox.is_empty() {
        return Err("target actor must expose inbox".to_string());
    }
    let following = owner_following_row(env, &local_actor.id, &remote.id)
        .await
        .map_err(|error| error.to_string())?;
    let recent_public_posts = fetch_actor_recent_public_posts(&remote).await;
    let handle = actor_handle(&remote);

    let mut response = Map::new();
    response.insert("id".to_string(), Value::String(remote.id));
    response.insert(
        "actor_type".to_string(),
        remote.actor_type.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert("inbox".to_string(), Value::String(remote.inbox));
    response.insert(
        "shared_inbox".to_string(),
        remote
            .shared_inbox
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    response.insert(
        "preferred_username".to_string(),
        remote
            .preferred_username
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    response.insert(
        "name".to_string(),
        remote.name.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "summary".to_string(),
        remote.summary.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "url".to_string(),
        remote.url.clone().map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "icon_url".to_string(),
        remote
            .icon_url
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    response.insert(
        "handle".to_string(),
        handle.map(Value::String).unwrap_or(Value::Null),
    );
    response.insert(
        "following_status".to_string(),
        following
            .as_ref()
            .and_then(|row| string_field(Some(row), "status"))
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    response.insert(
        "target_public_post".to_string(),
        target_public_post.map(Value::Object).unwrap_or(Value::Null),
    );
    response.insert(
        "recent_public_posts".to_string(),
        Value::Array(recent_public_posts.into_iter().map(Value::Object).collect()),
    );
    Ok(response)
}

async fn owner_unfollow_actor(
    env: &Env,
    target: &str,
) -> std::result::Result<Map<String, Value>, String> {
    if target.is_empty() {
        return Err("target is required".to_string());
    }
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let remote = resolve_activitypub_actor_for_local(target, &local_actor).await?;
    let existing = owner_following_row(env, &local_actor.id, &remote.id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "not currently following target".to_string())?;
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let undo_id = format!(
        "{}#undos/{}",
        local_actor.id,
        stable_id(&format!("{}\n{now}", remote.id))
            .chars()
            .take(16)
            .collect::<String>()
    );
    let existing_id = string_field(Some(&existing), "id").unwrap_or_default();
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Undo",
        "id": undo_id,
        "actor": local_actor.id,
        "object": {
            "type": "Follow",
            "id": existing_id,
            "actor": local_actor.id,
            "object": remote.id,
        },
        "to": [remote.id],
        "published": now,
    });

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let local_arg = D1Type::Text(&local_actor.id);
    let remote_arg = D1Type::Text(&remote.id);
    db.prepare(
        r#"
        UPDATE following
        SET status = 'rejected', accepted_at = NULL
        WHERE actor_id = ?1 AND target_actor_id = ?2
        "#,
    )
    .bind_refs([&local_arg, &remote_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let inbox = string_field(Some(&existing), "target_inbox").unwrap_or(remote.inbox);
    let delivery_ids = insert_delivery_rows(
        env,
        &undo_id,
        vec![inbox],
        "Undo",
        Some(activity.to_string()),
    )
    .await?;
    let following = owner_following_row(env, &local_actor.id, &remote.id)
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let mut response = Map::new();
    response.insert("ok".to_string(), Value::Bool(true));
    response.insert("following".to_string(), Value::Object(following));
    response.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(response)
}

async fn owner_following_row(
    env: &Env,
    actor_id: &str,
    target_actor_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let actor_arg = D1Type::Text(actor_id);
    let target_arg = D1Type::Text(target_actor_id);
    db.prepare(
        r#"
        SELECT id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
        FROM following
        WHERE actor_id = ?1 AND target_actor_id = ?2
        LIMIT 1
        "#,
    )
    .bind_refs([&actor_arg, &target_arg])?
    .first::<Map<String, Value>>(None)
    .await
}

async fn resolve_activitypub_object_inbox(object_id: &str) -> std::result::Result<String, String> {
    let object_url = public_https_url(object_id, "object_id")?;
    if let Some(inbox) = local_object_inbox(&object_url) {
        return Ok(inbox);
    }
    let object = fetch_activitypub_json(&object_url, "object").await?;
    let actor_id = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .and_then(optional_body_string)
        .ok_or_else(|| "object does not expose attributedTo or actor".to_string())?;
    let actor_url = public_https_url(&actor_id, "target")?;
    let actor = fetch_activitypub_json(&actor_url, "actor").await?;
    let inbox = actor
        .get("inbox")
        .and_then(optional_body_string)
        .unwrap_or_default();
    if inbox.is_empty() {
        return Err("object actor does not expose inbox".to_string());
    }
    let shared_inbox = actor
        .get("endpoints")
        .and_then(Value::as_object)
        .and_then(|endpoints| endpoints.get("sharedInbox"))
        .and_then(optional_body_string);
    Ok(shared_inbox.unwrap_or(inbox))
}

async fn resolve_activitypub_actor(target: &str) -> std::result::Result<RemoteActor, String> {
    let actor_url = activitypub_actor_url_for_target(target).await?;
    let actor = fetch_activitypub_json(&actor_url, "actor").await?;
    remote_actor_from_json(actor_url, actor)
}

async fn resolve_activitypub_actor_for_local(
    target: &str,
    local_actor: &LocalActor,
) -> std::result::Result<RemoteActor, String> {
    let actor_url = activitypub_actor_url_for_target(target).await?;
    let actor = match fetch_activitypub_json(&actor_url, "actor").await {
        Ok(actor) => actor,
        Err(unsigned_error)
            if should_retry_signed_fetch(&unsigned_error) && local_actor.can_sign() =>
        {
            fetch_activitypub_json_signed(&actor_url, "actor", local_actor)
                .await
                .map_err(|signed_error| {
                    format!("{unsigned_error}; signed retry failed: {signed_error}")
                })?
        }
        Err(unsigned_error) if should_retry_signed_fetch(&unsigned_error) => {
            return Err(format!(
                "{unsigned_error}; signed retry skipped: local actor signing key is not configured"
            ));
        }
        Err(error) => return Err(error),
    };
    remote_actor_from_json(actor_url, actor)
}

async fn activitypub_actor_url_for_target(target: &str) -> std::result::Result<String, String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        public_https_url(target, "target")
    } else {
        resolve_webfinger_actor(target).await
    }
}

fn remote_actor_from_json(
    actor_url: String,
    actor: Value,
) -> std::result::Result<RemoteActor, String> {
    let endpoints = actor.get("endpoints").and_then(Value::as_object);
    Ok(RemoteActor {
        id: actor
            .get("id")
            .and_then(optional_body_string)
            .unwrap_or_else(|| actor_url.clone()),
        actor_type: actor.get("type").and_then(optional_body_string),
        inbox: actor
            .get("inbox")
            .and_then(optional_body_string)
            .unwrap_or_default(),
        shared_inbox: endpoints
            .and_then(|value| value.get("sharedInbox"))
            .and_then(optional_body_string),
        preferred_username: actor
            .get("preferredUsername")
            .and_then(optional_body_string),
        name: actor
            .get("name")
            .and_then(optional_body_string)
            .or_else(|| {
                actor
                    .get("preferredUsername")
                    .and_then(optional_body_string)
            }),
        summary: actor.get("summary").and_then(optional_body_string),
        icon_url: actor
            .get("icon")
            .and_then(Value::as_object)
            .and_then(|icon| icon.get("url"))
            .and_then(optional_body_string),
        url: actor
            .get("url")
            .and_then(optional_body_string)
            .or(Some(actor_url)),
        outbox: actor.get("outbox").and_then(optional_body_string),
    })
}

fn should_retry_signed_fetch(error: &str) -> bool {
    error.contains("HTTP 401") || error.contains("HTTP 403")
}

async fn resolve_webfinger_actor(target: &str) -> std::result::Result<String, String> {
    let handle = target.trim().trim_start_matches('@');
    if !handle.contains('@') {
        return Err("target must be an actor URL or @user@domain handle".to_string());
    }
    let domain = handle.rsplit('@').next().unwrap_or_default().trim();
    public_https_url(&format!("https://{domain}/"), "target domain")?;
    let resource = format!("acct:{handle}");
    let url = format!(
        "https://{}/.well-known/webfinger?resource={}",
        domain,
        urlencoding::encode(&resource)
    );
    let jrd =
        fetch_json_with_accept(&url, "application/jrd+json, application/json", "webfinger").await?;
    let links = jrd
        .get("links")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("no ActivityPub self link found for {target}"))?;
    for link in links {
        let Some(object) = link.as_object() else {
            continue;
        };
        let rel = object
            .get("rel")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let link_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let href = object
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if rel == "self" && link_type.contains("activity+json") && !href.is_empty() {
            return public_https_url(href, "actor link");
        }
    }
    Err(format!("no ActivityPub self link found for {target}"))
}

async fn discover_public_post_target(target: &str) -> Option<Map<String, Value>> {
    if !target.starts_with("http://") && !target.starts_with("https://") {
        return None;
    }
    let object_url = public_https_url(target, "target public post").ok()?;
    let item = fetch_activitypub_json(&object_url, "object").await.ok()?;
    normalize_discovered_public_post(&item)
}

async fn fetch_actor_recent_public_posts(actor: &RemoteActor) -> Vec<Map<String, Value>> {
    let Some(outbox) = actor.outbox.as_deref() else {
        return Vec::new();
    };
    let Ok(outbox_url) = public_https_url(outbox, "actor outbox") else {
        return Vec::new();
    };
    let Ok(outbox) = fetch_activitypub_json(&outbox_url, "object").await else {
        return Vec::new();
    };
    let page = match outbox.get("first").and_then(|value| {
        value.as_str().map(ToOwned::to_owned).or_else(|| {
            value
                .as_object()
                .and_then(|object| object.get("id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
    }) {
        Some(page_url) => match public_https_url(&page_url, "actor outbox first page") {
            Ok(url) => fetch_activitypub_json(&url, "object")
                .await
                .unwrap_or_else(|_| outbox.clone()),
            Err(_) => outbox.clone(),
        },
        None => outbox.clone(),
    };
    let items = page
        .get("orderedItems")
        .or_else(|| page.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(normalize_discovered_public_post)
        .take(3)
        .collect()
}

async fn fetch_activitypub_json(url: &str, label: &str) -> std::result::Result<Value, String> {
    if let Some(value) = local_activitypub_fixture_value(url) {
        return Ok(value);
    }
    fetch_json_with_accept_and_headers(
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
        label,
        &[],
    )
    .await
}

async fn fetch_activitypub_json_signed(
    url: &str,
    label: &str,
    local_actor: &LocalActor,
) -> std::result::Result<Value, String> {
    if let Some(value) = local_activitypub_fixture_value(url) {
        return Ok(value);
    }
    let signed_headers = signed_activitypub_get_headers(url, local_actor)?;
    fetch_json_with_accept_and_headers(
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
        label,
        &signed_headers,
    )
    .await
}

fn local_activitypub_fixture_value(url: &str) -> Option<Value> {
    let parsed = worker::Url::parse(url).ok()?;
    match parsed.path() {
        "/__dais-fixtures/activitypub/actor" => {
            let public_key = fixture_public_key(&parsed)?;
            let actor_url = parsed.to_string();
            let name = parsed
                .query_pairs()
                .find(|(key, _)| key == "name")
                .map(|(_, value)| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "dais-s2s-fixture".to_string());
            Some(serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": actor_url,
                "type": "Application",
                "preferredUsername": name,
                "name": name,
                "inbox": format!("{}://{}/__dais-fixtures/activitypub/inbox", parsed.scheme(), parsed.host_str().unwrap_or_default()),
                "outbox": fixture_url_with_public_key(&parsed, "/__dais-fixtures/activitypub/outbox"),
                "publicKey": {
                    "id": format!("{actor_url}#main-key"),
                    "owner": actor_url,
                    "publicKeyPem": public_key,
                },
            }))
        }
        "/__dais-fixtures/activitypub/outbox" => {
            let post = local_activitypub_fixture_post_value(&parsed)?;
            let post_id = post.get("id").and_then(Value::as_str).unwrap_or_default();
            Some(serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": parsed.to_string(),
                "type": "OrderedCollection",
                "totalItems": 1,
                "orderedItems": [
                    {
                        "id": format!("{post_id}#create"),
                        "type": "Create",
                        "actor": post.get("attributedTo").cloned().unwrap_or(Value::Null),
                        "to": post.get("to").cloned().unwrap_or(Value::Array(Vec::new())),
                        "object": post,
                    }
                ],
            }))
        }
        "/__dais-fixtures/activitypub/posts/public-preview" => {
            local_activitypub_fixture_post_value(&parsed)
        }
        _ => None,
    }
}

fn local_activitypub_fixture_post_value(url: &worker::Url) -> Option<Value> {
    let post_id =
        fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/posts/public-preview");
    let object_type = url
        .query_pairs()
        .find(|(key, _)| key == "kind")
        .map(|(_, value)| value.to_string())
        .filter(|value| supported_timeline_object_type(value))
        .unwrap_or_else(|| "Note".to_string());
    let actor = fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/actor");
    let mut object = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": post_id,
        "type": object_type.clone(),
        "attributedTo": actor,
        "to": [PUBLIC_COLLECTION],
        "published": "2026-06-16T00:00:00Z",
        "url": post_id,
    });
    match object_type.as_str() {
        "Image" => {
            object["name"] = Value::String("Dais fixture public image".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an image server.".to_string());
            object["url"] = serde_json::json!([{
                "type": "Link",
                "mediaType": "image/png",
                "href": post_id,
            }]);
        }
        "Video" => {
            object["name"] = Value::String("Dais fixture public video".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from a video server.".to_string());
        }
        "Audio" => {
            object["name"] = Value::String("Dais fixture public audio".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an audio server.".to_string());
        }
        "Event" => {
            object["name"] = Value::String("Dais fixture public event".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an event server.".to_string());
            object["startTime"] = Value::String("2026-06-17T18:00:00Z".to_string());
            object["endTime"] = Value::String("2026-06-17T19:00:00Z".to_string());
            object["location"] = serde_json::json!({
                "type": "Place",
                "name": "Example venue",
            });
        }
        "Article" | "Page" | "Review" => {
            object["name"] = Value::String(format!("Dais fixture public {object_type}"));
            object["content"] = Value::String(format!(
                "<p>Dais fixture public preview post from a {} server.</p>",
                object_type.to_ascii_lowercase()
            ));
        }
        _ => {
            object["content"] =
                Value::String("<p>Dais fixture public preview post</p>".to_string());
        }
    }
    Some(object)
}

async fn fetch_json_with_accept(
    url: &str,
    accept: &str,
    label: &str,
) -> std::result::Result<Value, String> {
    fetch_json_with_accept_and_headers(url, accept, label, &[]).await
}

async fn fetch_lenient_json_with_accept(
    url: &str,
    accept: &str,
    label: &str,
) -> std::result::Result<Value, String> {
    let headers = Headers::new();
    headers
        .set("Accept", accept)
        .map_err(|error| error.to_string())?;
    headers
        .set("User-Agent", "dais-owner-api/1.0")
        .map_err(|error| error.to_string())?;
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status_code();
    if !(200..=299).contains(&status) {
        return Err(format!("could not fetch {label} {url}: HTTP {status}"));
    }
    let body = response.text().await.map_err(|error| error.to_string())?;
    parse_lenient_json_body(&body).map_err(|error| format!("could not parse {label} JSON: {error}"))
}

fn parse_lenient_json_body(body: &str) -> std::result::Result<Value, serde_json::Error> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed);
    }
    let json_start = trimmed
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '{' | '[').then_some(index))
        .unwrap_or(0);
    serde_json::from_str(&trimmed[json_start..])
}

async fn fetch_json_with_accept_and_headers(
    url: &str,
    accept: &str,
    label: &str,
    extra_headers: &[(String, String)],
) -> std::result::Result<Value, String> {
    let headers = Headers::new();
    headers
        .set("Accept", accept)
        .map_err(|error| error.to_string())?;
    headers
        .set("User-Agent", "dais-owner-api/1.0")
        .map_err(|error| error.to_string())?;
    for (name, value) in extra_headers {
        headers
            .set(name, value)
            .map_err(|error| error.to_string())?;
    }
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status_code();
    if !(200..=299).contains(&status) {
        return Err(format!("could not fetch {label} {url}: HTTP {status}"));
    }
    response
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())
}

fn signed_activitypub_get_headers(
    url: &str,
    local_actor: &LocalActor,
) -> std::result::Result<Vec<(String, String)>, String> {
    let parsed = worker::Url::parse(url).map_err(|error| error.to_string())?;
    let host = activitypub_request_host(&parsed)?;
    let request_target = activitypub_request_target(&parsed, &host);
    let date = js_sys::Date::new_0()
        .to_utc_string()
        .as_string()
        .unwrap_or_default();
    if date.is_empty() {
        return Err("could not generate Date header".to_string());
    }

    let mut sign_headers = HashMap::new();
    sign_headers.insert("host".to_string(), host.clone());
    sign_headers.insert("date".to_string(), date.clone());
    let headers_to_sign = vec![
        "(request-target)".to_string(),
        "host".to_string(),
        "date".to_string(),
    ];
    let key_id = format!("{}#main-key", local_actor.id);
    let signature = sign_request(
        &local_actor.private_key,
        &key_id,
        "GET",
        &request_target,
        &sign_headers,
        &headers_to_sign,
    )?;
    Ok(vec![
        ("Host".to_string(), host),
        ("Date".to_string(), date),
        ("Signature".to_string(), signature.to_header()),
    ])
}

fn activitypub_request_host(url: &worker::Url) -> std::result::Result<String, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "target URL is missing a host".to_string())?;
    match url.port() {
        Some(port) => Ok(format!("{host}:{port}")),
        None => Ok(host.to_string()),
    }
}

fn activitypub_request_target(url: &worker::Url, host: &str) -> String {
    let origin = format!("{}://{}", url.scheme(), host);
    url.to_string()
        .strip_prefix(&origin)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| url.path().to_string())
}

fn local_object_inbox(object_id: &str) -> Option<String> {
    let url = worker::Url::parse(object_id).ok()?;
    let mut parts = url.path().split('/').filter(|part| !part.is_empty());
    if parts.next()? != "users" {
        return None;
    }
    let username = parts.next()?;
    if parts.next()? != "posts" || parts.next().is_none() {
        return None;
    }
    Some(format!(
        "{}://{}/users/{}/inbox",
        url.scheme(),
        url.host_str()?,
        username
    ))
}

fn normalize_discovered_public_post(item: &Value) -> Option<Map<String, Value>> {
    let object = if item.get("type").and_then(Value::as_str) == Some("Create") {
        item.get("object").unwrap_or(item)
    } else {
        item
    };
    let object_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !supported_timeline_object_type(object_type) {
        return None;
    }
    let object_map = object.as_object()?;
    let mut recipients = Vec::new();
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(item.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    collect_recipients(item.get("cc"), &mut recipients);
    if !recipients.iter().any(|value| value == PUBLIC_COLLECTION) {
        return None;
    }
    let mut post = Map::new();
    post.insert(
        "id".to_string(),
        Value::String(
            object
                .get("id")
                .or_else(|| item.get("id"))
                .and_then(optional_body_string)
                .unwrap_or_default(),
        ),
    );
    post.insert("type".to_string(), Value::String(object_type.to_string()));
    post.insert(
        "actor_id".to_string(),
        public_post_actor_id(item, object)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "url".to_string(),
        object
            .get("url")
            .or_else(|| item.get("url"))
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "name".to_string(),
        object
            .get("name")
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "summary".to_string(),
        object
            .get("summary")
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    let content = activitypub_object_content_html(object_map);
    post.insert(
        "content".to_string(),
        Value::String(strip_html(&content).chars().take(280).collect()),
    );
    post.insert(
        "published".to_string(),
        object
            .get("published")
            .or_else(|| item.get("published"))
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    Some(post)
}

fn collect_recipients(value: Option<&Value>, recipients: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = optional_body_string(item) {
                    recipients.push(text);
                }
            }
        }
        Some(value) => {
            if let Some(text) = optional_body_string(value) {
                recipients.push(text);
            }
        }
        None => {}
    }
}

fn public_post_actor_id(item: &Value, object: &Value) -> Option<String> {
    let actor = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .or_else(|| item.get("actor"))
        .or_else(|| item.get("attributedTo"))?;
    match actor {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.iter().find_map(optional_body_string),
        _ => None,
    }
}

fn actor_handle(actor: &RemoteActor) -> Option<String> {
    let preferred_username = actor.preferred_username.as_deref()?;
    let url = worker::Url::parse(actor.url.as_deref().unwrap_or(&actor.id)).ok()?;
    Some(format!(
        "@{}@{}",
        preferred_username,
        url.host_str().unwrap_or_default()
    ))
}

async fn owner_federation_target_allowed(
    env: &Env,
    target_url: &str,
) -> std::result::Result<bool, String> {
    let settings = owner_settings(env)
        .await
        .map_err(|error| error.to_string())?;
    if !bool_field(Some(&settings), "closed_network") {
        return Ok(true);
    }
    let host = worker::Url::parse(target_url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.is_empty() {
        return Ok(false);
    }
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let host_arg = D1Type::Text(&host);
    let row = db
        .prepare(
            "SELECT 1 AS allowed FROM federation_allowlist WHERE host = ?1 AND enabled = 1 LIMIT 1",
        )
        .bind_refs(&host_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    Ok(row.is_some())
}

async fn owner_approved_follower_inboxes(env: &Env) -> Result<Vec<String>> {
    let db = env.d1("DB")?;
    let rows = db
        .prepare(
            r#"
            SELECT COALESCE(NULLIF(follower_shared_inbox, ''), follower_inbox) AS inbox
            FROM followers
            WHERE status = 'approved'
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|row| string_field(Some(&row), "inbox"))
        .collect())
}

fn normalize_source_item(row: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&row, "id"));
    item.insert("title".to_string(), row_value_or_null(&row, "title"));
    item.insert(
        "source_type".to_string(),
        row_value_or_null(&row, "source_type"),
    );
    item.insert(
        "canonical_url".to_string(),
        row_value_or_null(&row, "canonical_url"),
    );
    item.insert(
        "excerpt".to_string(),
        row_value_or_fallback_null(&row, "excerpt", "summary"),
    );
    item.insert(
        "rights_policy_json".to_string(),
        non_empty_value(&row, "rights_policy_json")
            .unwrap_or_else(|| Value::String("{}".to_string())),
    );
    item.insert(
        "read".to_string(),
        Value::Bool(bool_field(Some(&row), "read")),
    );
    item.insert(
        "source_id".to_string(),
        row_value_or_null(&row, "source_id"),
    );
    item.insert("author".to_string(), row_value_or_null(&row, "author"));
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&row, "published_at"),
    );
    item.insert(
        "fetched_at".to_string(),
        row_value_or_null(&row, "fetched_at"),
    );
    item.insert(
        "thumbnail_url".to_string(),
        row_value_or_null(&row, "thumbnail_url"),
    );
    item
}

async fn owner_local_actor(env: &Env) -> Result<LocalActor> {
    let db = env.d1("DB")?;
    let row = db
        .prepare("SELECT id, username, private_key FROM actors WHERE username = 'social' LIMIT 1")
        .first::<Map<String, Value>>(None)
        .await?;
    let private_key = env
        .secret("PRIVATE_KEY")
        .ok()
        .map(|secret| secret.to_string())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| string_field(row.as_ref(), "private_key"))
        .unwrap_or_default();
    Ok(LocalActor {
        id: string_field(row.as_ref(), "id").unwrap_or_else(|| local_actor_url(env)),
        private_key,
    })
}

fn query_param(url: &worker::Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

async fn read_json(req: &mut Request) -> Value {
    req.json::<Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({}))
}

async fn read_mastodon_body(req: &mut Request) -> Value {
    let content_type = request_content_type(req);
    if content_type.contains("application/json") {
        return read_json(req).await;
    }
    if content_type.contains("application/x-www-form-urlencoded") {
        let text = req.text().await.unwrap_or_default();
        let mut body = Map::new();
        for pair in text.split('&').filter(|part| !part.is_empty()) {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().map(decode_form_component).unwrap_or_default();
            if key.is_empty() {
                continue;
            }
            let value = parts.next().map(decode_form_component).unwrap_or_default();
            insert_repeating_body_value(&mut body, key, Value::String(value));
        }
        return Value::Object(body);
    }
    serde_json::json!({})
}

fn request_content_type(req: &Request) -> String {
    req.headers()
        .get("Content-Type")
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn decode_form_component(value: &str) -> String {
    decode_component(&value.replace('+', " "))
}

fn insert_repeating_body_value(body: &mut Map<String, Value>, key: String, value: Value) {
    match body.get_mut(&key) {
        Some(Value::Array(items)) => items.push(value),
        Some(existing) => {
            let previous = existing.clone();
            *existing = Value::Array(vec![previous, value]);
        }
        None => {
            body.insert(key, value);
        }
    }
}

fn required_body_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) if number.as_i64().unwrap_or(1) != 0 => {
            Some(number.to_string())
        }
        Some(Value::Bool(true)) => Some("true".to_string()),
        _ => None,
    }
}

fn string_like_field(body: &Value, key: &str) -> Option<String> {
    body.get(key).map(|value| match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => value.to_string(),
    })
}

fn string_like_any(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_like_field(body, key))
}

fn optional_trimmed_body(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(optional_body_string))
}

fn optional_body_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) if number.as_i64().unwrap_or(1) != 0 => Some(number.to_string()),
        Value::Bool(true) => Some("true".to_string()),
        _ => None,
    }
}

fn optional_url_field(
    body: &Value,
    key: &str,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    let Some(value) = body.get(key).and_then(optional_body_string) else {
        return Ok(None);
    };
    let url =
        worker::Url::parse(&value).map_err(|_| format!("{field} must be an absolute https URL"))?;
    if url.scheme() != "https" {
        return Err(format!("{field} must be an absolute https URL"));
    }
    Ok(Some(value))
}

fn media_r2_key_from_url(value: &str) -> Option<String> {
    let parsed = worker::Url::parse(value).ok()?;
    if !is_known_activitypub_host(parsed.host_str()) {
        return None;
    }
    let path = parsed.path();
    if let Some(rest) = path.strip_prefix("/media/_private/") {
        return Some(format!("private/{}", decode_component(rest)));
    }
    if let Some(rest) = path.strip_prefix("/media/_private_signed/") {
        return Some(format!("private/{}", decode_component(rest)));
    }
    if let Some(rest) = path.strip_prefix("/media/uploads/") {
        return Some(decode_component(&format!("uploads/{rest}")));
    }
    None
}

#[derive(Clone)]
struct SourcePolicy {
    private_reader_only: bool,
    excerpt_only: bool,
    link_required: bool,
    attribution_required: bool,
    no_image: bool,
    full_text_allowed: bool,
}

impl SourcePolicy {
    fn default() -> Self {
        Self {
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            no_image: false,
            full_text_allowed: false,
        }
    }

    fn to_value(&self) -> Value {
        serde_json::json!({
            "private_reader_only": self.private_reader_only,
            "excerpt_only": self.excerpt_only,
            "link_required": self.link_required,
            "attribution_required": self.attribution_required,
            "no_image": self.no_image,
            "full_text_allowed": self.full_text_allowed,
        })
    }
}

struct SourceRefreshItem {
    id: String,
    title: String,
    canonical_url: Option<String>,
    external_id: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    excerpt: Option<String>,
    thumbnail_url: Option<String>,
    hash: String,
}

fn source_policy_from_row(row: &Map<String, Value>) -> SourcePolicy {
    let mut policy = SourcePolicy::default();
    let Some(value) = string_field(Some(row), "policy_json") else {
        return policy;
    };
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&value) else {
        return policy;
    };
    if let Some(value) = object.get("private_reader_only").and_then(Value::as_bool) {
        policy.private_reader_only = value;
    }
    if let Some(value) = object.get("excerpt_only").and_then(Value::as_bool) {
        policy.excerpt_only = value;
    }
    if let Some(value) = object.get("link_required").and_then(Value::as_bool) {
        policy.link_required = value;
    }
    if let Some(value) = object.get("attribution_required").and_then(Value::as_bool) {
        policy.attribution_required = value;
    }
    if let Some(value) = object.get("no_image").and_then(Value::as_bool) {
        policy.no_image = value;
    }
    if let Some(value) = object.get("full_text_allowed").and_then(Value::as_bool) {
        policy.full_text_allowed = value;
    }
    policy
}

fn parse_feed_items(
    xml: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> Vec<SourceRefreshItem> {
    let rss_items = xml_blocks(xml, "item");
    if !rss_items.is_empty() {
        return rss_items
            .into_iter()
            .map(|block| normalize_feed_block(&block, source, policy, "rss"))
            .collect();
    }
    xml_blocks(xml, "entry")
        .into_iter()
        .map(|block| normalize_feed_block(&block, source, policy, "atom"))
        .collect()
}

fn parse_api_items(
    body: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let value = serde_json::from_str::<Value>(body).map_err(|error| error.to_string())?;
    let rows = value
        .get("articles")
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(rows
        .iter()
        .map(|row| normalize_api_item(row, source, policy))
        .collect())
}

async fn watch_activitypub_actor_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let remote = resolve_activitypub_actor(&target).await?;
    let posts = fetch_actor_recent_public_posts(&remote).await;
    Ok(posts
        .iter()
        .filter_map(|post| activitypub_watch_item(source, post, policy))
        .collect())
}

async fn watch_activitypub_object_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let object_url = public_https_url(&target, "watch target")?;
    let object = fetch_activitypub_json(&object_url, "watch object").await?;
    let Some(post) = normalize_discovered_public_post(&object) else {
        return Ok(Vec::new());
    };
    Ok(activitypub_watch_item(source, &post, policy)
        .into_iter()
        .collect())
}

async fn watch_bluesky_actor_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let actor = bluesky_actor_target(&target)?;
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=50&filter=posts_no_replies",
        urlencoding::encode(&actor)
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky author feed").await?;
    let feed = body
        .get("feed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(feed
        .iter()
        .filter_map(|row| row.get("post").or(Some(row)))
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

async fn watch_bluesky_post_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let uri = bluesky_post_uri(&target)?;
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.feed.getPostThread?uri={}&depth=1&parentHeight=0",
        urlencoding::encode(&uri)
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky post thread").await?;
    let mut posts = Vec::new();
    collect_bluesky_thread_posts(body.get("thread"), &mut posts);
    Ok(posts
        .iter()
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

fn activitypub_watch_item(
    source: &Map<String, Value>,
    post: &Map<String, Value>,
    policy: &SourcePolicy,
) -> Option<SourceRefreshItem> {
    let id = string_field(Some(post), "id")?;
    let canonical_url = string_field(Some(post), "url").or_else(|| Some(id.clone()));
    let title = string_field(Some(post), "name")
        .or_else(|| string_field(Some(post), "summary"))
        .or_else(|| string_field(Some(post), "content"))
        .map(|value| source_title(&strip_html(&value), "ActivityPub public post"))
        .unwrap_or_else(|| "ActivityPub public post".to_string());
    let excerpt = string_field(Some(post), "content")
        .or_else(|| string_field(Some(post), "summary"))
        .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let published_at = normalize_source_date(string_field(Some(post), "published"));
    Some(source_refresh_item(
        source,
        title,
        canonical_url,
        Some(id),
        string_field(Some(post), "actor_id"),
        published_at,
        excerpt,
        None,
    ))
}

fn bluesky_watch_item(
    source: &Map<String, Value>,
    post: &Value,
    policy: &SourcePolicy,
) -> Option<SourceRefreshItem> {
    let object = post.as_object()?;
    let uri = object.get("uri").and_then(optional_body_string)?;
    let author = object.get("author").and_then(Value::as_object);
    let handle = author
        .and_then(|row| row.get("handle"))
        .and_then(optional_body_string);
    let display_name = author
        .and_then(|row| row.get("displayName"))
        .and_then(optional_body_string);
    let author_label = display_name.or_else(|| handle.clone()).or_else(|| {
        author
            .and_then(|row| row.get("did"))
            .and_then(optional_body_string)
    });
    let record = object.get("record").and_then(Value::as_object);
    let text = record
        .and_then(|row| row.get("text"))
        .and_then(optional_body_string)
        .unwrap_or_default();
    let title = if text.trim().is_empty() {
        author_label
            .as_ref()
            .map(|author| format!("Bluesky public post by {author}"))
            .unwrap_or_else(|| "Bluesky public post".to_string())
    } else {
        source_title(&text, "Bluesky public post")
    };
    let canonical_url = bluesky_post_url(&uri, handle.as_deref()).or_else(|| Some(uri.clone()));
    let published_at = normalize_source_date(
        record
            .and_then(|row| row.get("createdAt"))
            .and_then(optional_body_string)
            .or_else(|| object.get("indexedAt").and_then(optional_body_string)),
    );
    let excerpt = source_excerpt(&text, excerpt_limit(policy));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        bluesky_post_thumbnail(post)
    };
    Some(source_refresh_item(
        source,
        title,
        canonical_url,
        Some(uri),
        author_label,
        published_at,
        excerpt,
        thumbnail_url,
    ))
}

fn collect_bluesky_thread_posts(value: Option<&Value>, posts: &mut Vec<Value>) {
    let Some(Value::Object(object)) = value else {
        return;
    };
    if let Some(post) = object.get("post") {
        posts.push(post.clone());
    }
    if let Some(replies) = object.get("replies").and_then(Value::as_array) {
        for reply in replies {
            collect_bluesky_thread_posts(Some(reply), posts);
        }
    }
}

fn bluesky_post_thumbnail(post: &Value) -> Option<String> {
    let embed = post.get("embed").and_then(Value::as_object)?;
    embed
        .get("images")
        .and_then(Value::as_array)
        .and_then(|images| images.first())
        .and_then(|image| {
            image
                .get("thumb")
                .or_else(|| image.get("fullsize"))
                .and_then(optional_body_string)
        })
        .or_else(|| {
            embed
                .get("external")
                .and_then(Value::as_object)
                .and_then(|external| external.get("thumb"))
                .and_then(optional_body_string)
        })
}

fn source_title(value: &str, fallback: &str) -> String {
    let text = collapse_whitespace(value);
    if text.is_empty() {
        fallback.to_string()
    } else {
        text.chars().take(120).collect()
    }
}

fn normalize_api_item(
    row: &Value,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> SourceRefreshItem {
    let title = value_string(row.get("title"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(untitled source item)".to_string());
    let canonical_url = value_string(row.get("url").or_else(|| row.get("external_url")));
    let external_id = value_string(row.get("id").or_else(|| row.get("guid")))
        .or_else(|| canonical_url.clone())
        .or_else(|| Some(title.clone()));
    let author = value_string(row.get("author").or_else(|| row.get("byline"))).or_else(|| {
        row.get("source")
            .and_then(|source| source.get("name"))
            .and_then(|value| value_string(Some(value)))
    });
    let published_at = normalize_source_date(value_string(
        row.get("publishedAt")
            .or_else(|| row.get("date_published"))
            .or_else(|| row.get("published_at")),
    ));
    let excerpt = value_string(
        row.get("description")
            .or_else(|| row.get("summary"))
            .or_else(|| row.get("excerpt")),
    )
    .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        value_string(row.get("urlToImage").or_else(|| row.get("image")))
    };
    source_refresh_item(
        source,
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
    )
}

fn normalize_feed_block(
    block: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
    kind: &str,
) -> SourceRefreshItem {
    let title =
        xml_text_tag(block, "title").unwrap_or_else(|| "(untitled source item)".to_string());
    let canonical_url = if kind == "atom" {
        xml_attr_tag(block, "link", "href").or_else(|| xml_text_tag(block, "link"))
    } else {
        xml_text_tag(block, "link")
    };
    let external_id = xml_text_tag(block, "guid")
        .or_else(|| xml_text_tag(block, "id"))
        .or_else(|| canonical_url.clone())
        .or_else(|| Some(title.clone()));
    let author = xml_text_tag(block, "author")
        .or_else(|| xml_text_tag(block, "dc:creator"))
        .or_else(|| xml_text_tag(block, "name"));
    let published_at = normalize_source_date(
        xml_text_tag(block, "pubDate")
            .or_else(|| xml_text_tag(block, "published"))
            .or_else(|| xml_text_tag(block, "updated")),
    );
    let excerpt = xml_text_tag(block, "description")
        .or_else(|| xml_text_tag(block, "summary"))
        .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        xml_attr_tag(block, "media:thumbnail", "url")
    };
    source_refresh_item(
        source,
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
    )
}

fn source_refresh_item(
    source: &Map<String, Value>,
    title: String,
    canonical_url: Option<String>,
    external_id: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    excerpt: Option<String>,
    thumbnail_url: Option<String>,
) -> SourceRefreshItem {
    let source_id = string_field(Some(source), "id").unwrap_or_default();
    let external_seed = external_id.clone().unwrap_or_default();
    let canonical_seed = canonical_url.clone().unwrap_or_default();
    let seed = format!("{source_id}\n{external_seed}\n{canonical_seed}\n{title}");
    let hash = stable_id(&seed);
    SourceRefreshItem {
        id: format!("src-{}", hash.chars().take(24).collect::<String>()),
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
        hash,
    }
}

fn xml_blocks(xml: &str, tag: &str) -> Vec<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let close_tag = format!("</{}>", tag.to_ascii_lowercase());
    let mut blocks = Vec::new();
    let mut offset = 0;
    while let Some(open_rel) = lower_xml[offset..].find(&open_prefix) {
        let open = offset + open_rel;
        let Some(open_end_rel) = lower_xml[open..].find('>') else {
            break;
        };
        let content_start = open + open_end_rel + 1;
        let Some(close_rel) = lower_xml[content_start..].find(&close_tag) else {
            break;
        };
        let close = content_start + close_rel;
        blocks.push(xml[content_start..close].to_string());
        offset = close + close_tag.len();
    }
    blocks
}

fn xml_text_tag(xml: &str, tag: &str) -> Option<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let open = lower_xml.find(&open_prefix)?;
    let open_end = open + lower_xml[open..].find('>')?;
    let content_start = open_end + 1;
    let close_tag = format!("</{}>", tag.to_ascii_lowercase());
    let close = content_start + lower_xml[content_start..].find(&close_tag)?;
    let value = strip_xml_tags(&strip_cdata(&xml[content_start..close]));
    let decoded = decode_xml(value.trim());
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn xml_attr_tag(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let open = lower_xml.find(&open_prefix)?;
    let end = open + lower_xml[open..].find('>')?;
    let raw_attrs = &xml[open..end];
    let lower_attrs = raw_attrs.to_ascii_lowercase();
    let attr_prefix = format!("{}=", attr.to_ascii_lowercase());
    let attr_start = lower_attrs.find(&attr_prefix)? + attr_prefix.len();
    let quote = raw_attrs[attr_start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_start = attr_start + quote.len_utf8();
    let value_end = value_start + raw_attrs[value_start..].find(quote)?;
    Some(decode_xml(&raw_attrs[value_start..value_end])).filter(|value| !value.trim().is_empty())
}

fn strip_cdata(value: &str) -> String {
    value
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(value)
        .to_string()
}

fn strip_xml_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => {
                in_tag = true;
                output.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn source_excerpt(value: &str, max_chars: usize) -> Option<String> {
    let text = collapse_whitespace(&strip_xml_tags(&decode_xml(value)));
    let excerpt: String = text.chars().take(max_chars).collect();
    if excerpt.trim().is_empty() {
        None
    } else {
        Some(excerpt)
    }
}

fn excerpt_limit(policy: &SourcePolicy) -> usize {
    if policy.full_text_allowed && !policy.excerpt_only {
        2000
    } else {
        800
    }
}

fn normalize_source_date(value: Option<String>) -> Option<String> {
    let value = value?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        return Some(value);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&JsValue::from_str(&value));
        let millis = date.get_time();
        if millis.is_nan() {
            None
        } else {
            date.to_iso_string().as_string()
        }
    }
}

fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn row_int(row: &Map<String, Value>, key: &str) -> Option<i32> {
    match row.get(key)? {
        Value::Number(number) => number.as_i64().and_then(|value| i32::try_from(value).ok()),
        Value::String(text) => text.parse::<i32>().ok(),
        Value::Bool(value) => Some(if *value { 1 } else { 0 }),
        _ => None,
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn normalize_visibility(value: &str) -> Option<String> {
    let normalized = value.to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "public" | "unlisted" | "followers" | "direct"
    ) {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_protocol(value: &str) -> Option<String> {
    let normalized = value.to_ascii_lowercase().replace('_', "").replace('-', "");
    match normalized.as_str() {
        "activitypub" => Some("activitypub".to_string()),
        "atproto" => Some("atproto".to_string()),
        "both" => Some("both".to_string()),
        _ => None,
    }
}

fn normalize_attachments(values: &[Value]) -> std::result::Result<Vec<Value>, String> {
    let mut attachments = Vec::new();
    for value in values {
        let attachment = match value {
            Value::String(text) if text.trim().starts_with('{') => {
                serde_json::from_str::<Value>(text)
                    .map_err(|_| "attachment JSON is invalid".to_string())?
            }
            Value::String(text) => {
                serde_json::json!({ "type": "Document", "url": text.trim() })
            }
            Value::Object(_) => value.clone(),
            _ => return Err("attachment must be a URL or object".to_string()),
        };
        let Some(object) = attachment.as_object() else {
            return Err("attachment must be a URL or object".to_string());
        };
        let url = optional_https_url(object.get("url"), "attachment url")?;
        let media_type = object.get("mediaType").and_then(optional_body_string);
        if let Some(media_type) = media_type.as_deref() {
            if !allowed_media_type(media_type) {
                return Err("unsupported attachment media type".to_string());
            }
        }
        let mut normalized = Map::new();
        normalized.insert(
            "type".to_string(),
            Value::String(
                object
                    .get("type")
                    .and_then(optional_body_string)
                    .unwrap_or_else(|| {
                        if media_type
                            .as_deref()
                            .map(|value| value.starts_with("image/"))
                            .unwrap_or(false)
                        {
                            "Image".to_string()
                        } else {
                            "Document".to_string()
                        }
                    }),
            ),
        );
        normalized.insert(
            "url".to_string(),
            url.map(Value::String).unwrap_or(Value::Null),
        );
        if let Some(media_type) = media_type {
            normalized.insert("mediaType".to_string(), Value::String(media_type));
        }
        if let Some(name) = object.get("name").and_then(optional_body_string) {
            normalized.insert("name".to_string(), Value::String(name));
        }
        attachments.push(Value::Object(normalized));
    }
    Ok(attachments)
}

fn normalize_encrypted_media_attachments(
    values: &[Value],
) -> std::result::Result<Vec<Value>, String> {
    let mut attachments = Vec::new();
    for value in values {
        let attachment = match value {
            Value::String(text) if text.trim().starts_with('{') => {
                serde_json::from_str::<Value>(text)
                    .map_err(|_| "encrypted media attachment JSON is invalid".to_string())?
            }
            Value::Object(_) => value.clone(),
            _ => {
                return Err(
                    "encrypted media attachments must be ciphertext JSON objects".to_string(),
                )
            }
        };
        let object = attachment
            .as_object()
            .ok_or_else(|| "encrypted media attachment must be an object".to_string())?;
        if object.get("url").is_some()
            || object.get("data_base64").is_some()
            || object.get("dataBase64").is_some()
        {
            return Err(
                "encrypted media attachments must not include plaintext bytes or fetch URLs"
                    .to_string(),
            );
        }
        let encrypted_media = object
            .get("encryptedMedia")
            .ok_or_else(|| "encrypted media attachment requires encryptedMedia".to_string())?;
        validate_encrypted_media_payload(encrypted_media)?;

        let media_type = encrypted_media
            .get("mediaType")
            .or_else(|| object.get("mediaType"))
            .and_then(Value::as_str)
            .unwrap_or("application/octet-stream");
        if media_type != "application/octet-stream" && !allowed_media_type(media_type) {
            return Err("unsupported encrypted attachment media type".to_string());
        }
        let mut normalized = Map::new();
        normalized.insert(
            "type".to_string(),
            Value::String(
                object
                    .get("type")
                    .and_then(optional_body_string)
                    .unwrap_or_else(|| {
                        if media_type.starts_with("image/") {
                            "Image".to_string()
                        } else {
                            "Document".to_string()
                        }
                    }),
            ),
        );
        normalized.insert(
            "mediaType".to_string(),
            Value::String(media_type.to_string()),
        );
        if let Some(name) = object
            .get("name")
            .or_else(|| encrypted_media.get("name"))
            .and_then(optional_body_string)
            .map(|name| name.chars().take(160).collect::<String>())
        {
            normalized.insert("name".to_string(), Value::String(name));
        }
        normalized.insert("encryptedMedia".to_string(), encrypted_media.clone());
        attachments.push(Value::Object(normalized));
    }
    Ok(attachments)
}

fn encrypted_media_attachments_from_activitypub_object(
    object: &Value,
) -> std::result::Result<Vec<Value>, String> {
    let values = match object.get("attachment") {
        Some(Value::Array(values)) => values.clone(),
        Some(value) => vec![value.clone()],
        None => Vec::new(),
    };
    normalize_encrypted_media_attachments(&values)
}

fn optional_https_url(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    let Some(value) = value.and_then(optional_body_string) else {
        return Ok(None);
    };
    let url =
        worker::Url::parse(&value).map_err(|_| format!("{field} must be an absolute https URL"))?;
    if url.scheme() != "https" {
        return Err(format!("{field} must be an absolute https URL"));
    }
    Ok(Some(value))
}

fn is_private_media_attachment(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("url"))
        .and_then(Value::as_str)
        .and_then(|url| worker::Url::parse(url).ok())
        .map(|url| {
            is_known_activitypub_host(url.host_str())
                && (url.path().starts_with("/media/_private/")
                    || url.path().starts_with("/media/_private_signed/"))
        })
        .unwrap_or(false)
}

fn is_public_atproto_image_attachment(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let media_type_is_image = object
        .get("mediaType")
        .and_then(Value::as_str)
        .map(|value| value.starts_with("image/"))
        .unwrap_or(false);
    if !media_type_is_image {
        return false;
    }
    !is_private_media_attachment(value)
        && object
            .get("url")
            .and_then(Value::as_str)
            .and_then(|url| worker::Url::parse(url).ok())
            .is_some_and(|url| url.scheme() == "https")
}

fn is_local_object_url(value: &str, local_host: &str) -> bool {
    worker::Url::parse(value)
        .ok()
        .map(|url| url.host_str() == Some(local_host) && url.path().starts_with("/users/social/"))
        .unwrap_or(false)
}

fn canonical_mastodon_status_id(value: &str) -> String {
    worker::Url::parse(value)
        .ok()
        .and_then(|url| {
            let path = url.path();
            (path.starts_with("/users/social/posts/") && !path.ends_with('/')).then(|| {
                format!(
                    "{}://{}{}",
                    url.scheme(),
                    url.host_str().unwrap_or_default(),
                    path
                )
            })
        })
        .unwrap_or_else(|| value.to_string())
}

fn is_known_activitypub_host(host: Option<&str>) -> bool {
    matches!(host, Some("social.dais.social") | Some("social.skpt.cl"))
}

fn timestamp_for_local_id(iso: &str) -> String {
    iso.chars()
        .filter(|ch| !matches!(ch, '-' | ':' | 'T' | 'Z' | '.'))
        .take(14)
        .collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut previous_space = false;
    for ch in value.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !previous_space && !output.is_empty() {
                    output.push(' ');
                    previous_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            _ if ch.is_whitespace() => {
                if !previous_space && !output.is_empty() {
                    output.push(' ');
                    previous_space = true;
                }
            }
            _ => {
                output.push(ch);
                previous_space = false;
            }
        }
    }
    output.trim().to_string()
}

fn media_type_for_filename(filename: &str) -> String {
    match filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn allowed_media_type(value: &str) -> bool {
    matches!(
        value,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "video/mp4" | "video/webm"
    )
}

fn safe_media_filename(value: &str) -> std::result::Result<String, String> {
    let basename = value.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let mut safe = String::new();
    let mut previous_dash = false;
    for ch in basename.chars() {
        let replacement = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            ch
        } else {
            '-'
        };
        if replacement == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        safe.push(replacement);
    }
    let safe = safe
        .trim_start_matches('.')
        .chars()
        .take(96)
        .collect::<String>();
    if safe.is_empty() {
        return Err("filename is invalid".to_string());
    }
    Ok(safe)
}

fn private_media_expires_at(value: Option<&Value>) -> std::result::Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() || matches!(value, Value::String(text) if text.is_empty()) {
        return Ok(None);
    }
    let seconds = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
    .ok_or_else(|| "expires_in_seconds must be a positive number".to_string())?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("expires_in_seconds must be a positive number".to_string());
    }
    if seconds > 30.0 * 24.0 * 60.0 * 60.0 {
        return Err("expires_in_seconds must be 30 days or less".to_string());
    }
    let expires_ms = js_sys::Date::now() + seconds.floor() * 1000.0;
    Ok(js_sys::Date::new(&JsValue::from_f64(expires_ms))
        .to_iso_string()
        .as_string())
}

struct MediaMetadataInput<'a> {
    owner: &'a str,
    access: &'a str,
    media_type: &'a str,
    bytes: &'a [u8],
    created_at: &'a str,
    description: Option<&'a str>,
    expires_at: Option<&'a str>,
    require_authorized_fetch: bool,
}

fn media_custom_metadata(input: MediaMetadataInput<'_>) -> HashMap<String, String> {
    let mut custom_metadata = HashMap::new();
    custom_metadata.insert("owner".to_string(), input.owner.to_string());
    custom_metadata.insert("visibility".to_string(), input.access.to_string());
    custom_metadata.insert("media_type".to_string(), input.media_type.to_string());
    custom_metadata.insert("size".to_string(), input.bytes.len().to_string());
    custom_metadata.insert("sha256".to_string(), sha256_hex(input.bytes));
    custom_metadata.insert("created_at".to_string(), input.created_at.to_string());
    if let Some(description) = input.description {
        custom_metadata.insert("description".to_string(), description.to_string());
    }
    if let Some(expires_at) = input.expires_at {
        custom_metadata.insert("expires_at".to_string(), expires_at.to_string());
    }
    if input.require_authorized_fetch {
        custom_metadata.insert("authorized_fetch".to_string(), "required".to_string());
    }
    custom_metadata
}

fn media_metadata_is_expired(metadata: &HashMap<String, String>, now_ms: f64) -> bool {
    let Some(expires_at) = metadata.get("expires_at").map(String::as_str) else {
        return false;
    };
    let expires_ms = js_sys::Date::parse(expires_at);
    expires_ms.is_finite() && expires_ms <= now_ms
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn current_media_timestamp() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
        .chars()
        .filter(|ch| !matches!(ch, '-' | ':' | 'T' | 'Z' | '.'))
        .take(14)
        .collect()
}

fn current_media_created_at() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

fn random_token() -> std::result::Result<String, String> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| "crypto is unavailable".to_string())?;
    let get_random_values = js_sys::Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .map_err(|_| "crypto.getRandomValues is unavailable".to_string())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "crypto.getRandomValues is unavailable".to_string())?;
    let array = js_sys::Uint8Array::new_with_length(24);
    get_random_values
        .call1(&crypto, &array)
        .map_err(|_| "crypto.getRandomValues failed".to_string())?;
    let mut bytes = vec![0; 24];
    array.copy_to(&mut bytes);
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn clamp_cadence_minutes(value: Option<String>) -> i32 {
    let minutes = value
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(60.0);
    minutes.max(5.0).min(1440.0) as i32
}

fn source_policy_json_for_type(body: &Value, source_type: &str) -> String {
    let is_watch = is_watch_source_type(source_type);
    format!(
        "{{\"private_reader_only\":{},\"excerpt_only\":{},\"link_required\":{},\"attribution_required\":{},\"image_allowed\":{},\"full_text_allowed\":{},\"watch\":{},\"public_only\":{},\"no_remote_relationship\":{}}}",
        source_policy_default_true(body, "private_reader_only", "privateReaderOnly") || is_watch,
        source_policy_default_true(body, "excerpt_only", "excerptOnly"),
        source_policy_default_true(body, "link_required", "linkRequired"),
        source_policy_default_true(body, "attribution_required", "attributionRequired"),
        source_policy_bool(body, "image_allowed", "imageAllowed"),
        source_policy_bool(body, "full_text_allowed", "fullTextAllowed"),
        is_watch,
        is_watch,
        is_watch,
    )
}

fn source_policy_default_true(body: &Value, snake: &str, camel: &str) -> bool {
    !matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(false))
    )
}

fn source_policy_bool(body: &Value, snake: &str, camel: &str) -> bool {
    matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(true))
    )
}

fn normalize_source_type(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(':', "_")
}

fn addable_source_types() -> Vec<&'static str> {
    SOURCE_TYPES
        .iter()
        .copied()
        .filter(|value| *value != "activitypub")
        .collect()
}

fn is_addable_source_type(value: &str) -> bool {
    addable_source_types().iter().any(|item| *item == value)
}

fn is_watch_source_type(value: &str) -> bool {
    WATCH_SOURCE_TYPES.iter().any(|item| *item == value)
}

fn is_refreshable_source_type(value: &str) -> bool {
    REFRESHABLE_SOURCE_TYPES.iter().any(|item| *item == value)
}

fn source_type_for_watch_kind(value: &str) -> Option<&'static str> {
    match normalize_source_type(value).as_str() {
        "rss" | "feed" | "watch_rss" => Some("watch_rss"),
        "atom" | "watch_atom" => Some("watch_atom"),
        "activitypub" | "activitypub_actor" | "ap" | "actor" | "watch_activitypub_actor" => {
            Some("watch_activitypub_actor")
        }
        "activitypub_object"
        | "activitypub_post"
        | "ap_object"
        | "ap_post"
        | "watch_activitypub_object" => Some("watch_activitypub_object"),
        "bluesky"
        | "bsky"
        | "atproto"
        | "bluesky_actor"
        | "atproto_actor"
        | "watch_bluesky_actor" => Some("watch_bluesky_actor"),
        "bluesky_post" | "bsky_post" | "atproto_post" | "watch_bluesky_post" => {
            Some("watch_bluesky_post")
        }
        _ => None,
    }
}

fn normalized_source_target(
    source_type: &str,
    body: &Value,
) -> std::result::Result<String, String> {
    let raw = string_like_any(
        body,
        &["url", "target", "uri", "actor", "feed_url", "feedUrl"],
    )
    .unwrap_or_default();
    match source_type {
        "watch_activitypub_actor" => normalized_activitypub_actor_target(&raw),
        "watch_bluesky_actor" => bluesky_actor_target(&raw),
        "watch_bluesky_post" => bluesky_post_uri(&raw),
        "watch_rss" | "watch_atom" | "watch_activitypub_object" => {
            public_https_url(&raw, "watch target")
        }
        _ => public_https_url(&raw, "source url"),
    }
}

fn normalized_activitypub_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with('@') && trimmed.trim_start_matches('@').contains('@') {
        return Ok(trimmed.to_string());
    }
    public_https_url(trimmed, "watch target")
}

fn bluesky_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return Err("watch target is required".to_string());
    }
    if trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    if trimmed.starts_with("at://") {
        return trimmed
            .trim_start_matches("at://")
            .split('/')
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Bluesky actor target is invalid".to_string());
    }
    if let Ok(url) = worker::Url::parse(trimmed) {
        if url.host_str() != Some("bsky.app") {
            return Err("Bluesky actor URL must be on bsky.app".to_string());
        }
        let mut parts = url.path().split('/').filter(|part| !part.is_empty());
        if parts.next() == Some("profile") {
            if let Some(actor) = parts.next().filter(|value| !value.trim().is_empty()) {
                return Ok(actor.to_string());
            }
        }
        return Err(
            "Bluesky actor URL must look like https://bsky.app/profile/<handle-or-did>".to_string(),
        );
    }
    if trimmed.contains('.') || trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    Err("Bluesky actor target must be a handle, DID, or bsky.app profile URL".to_string())
}

fn bluesky_post_uri(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with("at://") && trimmed.contains("/app.bsky.feed.post/") {
        return Ok(trimmed.to_string());
    }
    let url = worker::Url::parse(trimmed)
        .map_err(|_| "Bluesky post target must be an at:// URI or bsky.app post URL".to_string())?;
    if url.host_str() != Some("bsky.app") {
        return Err("Bluesky post URL must be on bsky.app".to_string());
    }
    let parts: Vec<&str> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 4 && parts[0] == "profile" && parts[2] == "post" {
        return Ok(format!("at://{}/app.bsky.feed.post/{}", parts[1], parts[3]));
    }
    Err(
        "Bluesky post URL must look like https://bsky.app/profile/<handle-or-did>/post/<rkey>"
            .to_string(),
    )
}

fn source_id(source_type: &str, source_url: &str) -> String {
    let digest = Sha256::digest(format!("{source_type}\n{source_url}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("source-{}", &hex[..24])
}

fn body_string_any(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| required_body_string(body.get(*key)))
}

fn body_string_array_any(body: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| body.get(*key).map(array_from_body_value))
        .unwrap_or_default()
}

fn normalize_host(value: &str) -> Result<String> {
    normalize_host_value(value).map_err(|message| worker::Error::RustError(message.to_string()))
}

fn normalize_host_value(value: &str) -> std::result::Result<String, &'static str> {
    let raw = value.trim();
    if raw.is_empty() {
        return Err("host is required");
    }
    let lower_raw = raw.to_ascii_lowercase();
    let without_scheme = lower_raw
        .strip_prefix("http://")
        .or_else(|| lower_raw.strip_prefix("https://"))
        .unwrap_or(lower_raw.as_str());
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !host.contains('.')
        || host.is_empty()
        || !host.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
    {
        return Err("host must be a domain name");
    }
    if host_not_allowed(&host) {
        return Err("host is not allowed");
    }
    Ok(host)
}

fn public_https_url(value: &str, field: &str) -> std::result::Result<String, String> {
    let url = worker::Url::parse(value).map_err(|_| format!("{field} must be a valid URL"))?;
    if url.scheme() != "https" {
        return Err(format!("{field} must use https"));
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host_not_allowed(&host) || host == "::1" {
        return Err(format!("{field} host is not allowed"));
    }
    Ok(url.to_string())
}

fn normalize_e2ee_device_id(value: &str) -> std::result::Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err("deviceId is required".to_string());
    }
    if normalized.len() > 128 {
        return Err("deviceId is too long".to_string());
    }
    if !normalized
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(
            "deviceId may only contain letters, numbers, dot, colon, dash, and underscore"
                .to_string(),
        );
    }
    Ok(normalized.to_string())
}

fn normalize_e2ee_protocol(value: &str) -> std::result::Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "dais-mls-v1" | "encryptedmessage-v1" | "encrypted-message-v1" => {
            Ok("dais-mls-v1".to_string())
        }
        "mls" | "openmls" | "mls-rfc9420" | "openmls-rfc9420" | "dais-mls-v2" => {
            Ok("mls-rfc9420".to_string())
        }
        _ => Err("unsupported E2EE protocol".to_string()),
    }
}

fn normalize_e2ee_fingerprint(value: &str) -> std::result::Result<String, String> {
    let normalized = value
        .trim()
        .trim_start_matches("sha256:")
        .replace([':', ' ', '-'], "")
        .to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("fingerprint must be a SHA-256 hex digest".to_string());
    }
    Ok(normalized)
}

fn required_e2ee_material(
    body: &Value,
    keys: &[&str],
    field: &str,
) -> std::result::Result<String, String> {
    let value = body_string_any(body, keys).ok_or_else(|| format!("{field} is required"))?;
    if value.len() > 65536 {
        return Err(format!("{field} is too large"));
    }
    Ok(value)
}

fn validate_e2ee_device_material(
    protocol: &str,
    credential: &str,
    key_package: &str,
) -> std::result::Result<(), String> {
    if protocol == "mls-rfc9420" {
        let credential_bytes = BASE64
            .decode(credential.as_bytes())
            .map_err(|_| "MLS credential must be base64".to_string())?;
        let key_package_bytes = BASE64
            .decode(key_package.as_bytes())
            .map_err(|_| "MLS keyPackage must be base64".to_string())?;
        if credential_bytes.is_empty() {
            return Err("MLS credential must not be empty".to_string());
        }
        if key_package_bytes.is_empty() {
            return Err("MLS keyPackage must not be empty".to_string());
        }
    }
    Ok(())
}

fn validate_owner_e2ee_payload(
    value: &Value,
) -> std::result::Result<(&'static str, &'static str), String> {
    if value.get("protocol").and_then(Value::as_str) == Some("mls-rfc9420")
        || value.get("v").and_then(Value::as_u64) == Some(2)
    {
        validate_dais_encrypted_message_v2(value)?;
        Ok(("daisEncryptedMessage", "mls-rfc9420"))
    } else {
        validate_encrypted_message_envelope(value)?;
        Ok(("encryptedMessage", "dais-mls-v1"))
    }
}

fn validate_dais_encrypted_message_v2(value: &Value) -> std::result::Result<(), String> {
    let envelope = value
        .as_object()
        .ok_or_else(|| "daisEncryptedMessage must be an object".to_string())?;
    match envelope.get("v").and_then(Value::as_u64) {
        Some(2) => {}
        Some(version) => {
            return Err(format!(
                "unsupported daisEncryptedMessage version {version}"
            ))
        }
        None => return Err("daisEncryptedMessage.v is required".to_string()),
    }
    match envelope.get("protocol").and_then(Value::as_str) {
        Some("mls-rfc9420") => {}
        Some(_) => return Err("daisEncryptedMessage.protocol must be mls-rfc9420".to_string()),
        None => return Err("daisEncryptedMessage.protocol is required".to_string()),
    }
    required_nonempty_string(envelope, "groupId", "daisEncryptedMessage", 512)?;
    let epoch = envelope
        .get("epoch")
        .and_then(Value::as_u64)
        .ok_or_else(|| "daisEncryptedMessage.epoch is required".to_string())?;
    if epoch > i32::MAX as u64 {
        return Err("daisEncryptedMessage.epoch is too large".to_string());
    }
    normalize_e2ee_device_id(&required_nonempty_string(
        envelope,
        "senderDeviceId",
        "daisEncryptedMessage",
        128,
    )?)?;
    if required_dais_mls_base64(envelope, "ciphertext")?.is_empty() {
        return Err("daisEncryptedMessage.ciphertext must not be empty".to_string());
    }
    Ok(())
}

fn required_nonempty_string(
    object: &Map<String, Value>,
    key: &str,
    prefix: &str,
    max_len: usize,
) -> std::result::Result<String, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{prefix}.{key} is required"))?;
    if value.len() > max_len {
        return Err(format!("{prefix}.{key} is too long"));
    }
    Ok(value.to_string())
}

fn required_dais_mls_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = required_nonempty_string(object, key, "daisEncryptedMessage", 262144)?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("daisEncryptedMessage.{key} must be valid base64"))
}

fn validate_encrypted_message_envelope(value: &Value) -> std::result::Result<(), String> {
    let envelope = value
        .as_object()
        .ok_or_else(|| "encryptedMessage must be an object".to_string())?;
    match envelope.get("v").and_then(Value::as_u64) {
        Some(1) => {}
        Some(version) => return Err(format!("unsupported encryptedMessage version {version}")),
        None => return Err("encryptedMessage.v is required".to_string()),
    }
    match envelope.get("alg").and_then(Value::as_str) {
        Some("AES-256-GCM") => {}
        Some(_) => return Err("encryptedMessage.alg must be AES-256-GCM".to_string()),
        None => return Err("encryptedMessage.alg is required".to_string()),
    }
    match envelope.get("keyWrap").and_then(Value::as_str) {
        Some("RSA-OAEP-256") | Some("RSA-OAEP-SHA256") => {}
        Some(_) => {
            return Err(
                "encryptedMessage.keyWrap must be RSA-OAEP-256 or RSA-OAEP-SHA256".to_string(),
            )
        }
        None => return Err("encryptedMessage.keyWrap is required".to_string()),
    }
    let iv = required_encrypted_base64(envelope, "iv")?;
    if iv.len() != 12 {
        return Err("encryptedMessage.iv must decode to 12 bytes".to_string());
    }
    if required_encrypted_base64(envelope, "ciphertext")?.is_empty() {
        return Err("encryptedMessage.ciphertext must not be empty".to_string());
    }
    let recipients = envelope
        .get("recipients")
        .and_then(Value::as_array)
        .ok_or_else(|| "encryptedMessage.recipients must be an array".to_string())?;
    if recipients.is_empty() {
        return Err("encryptedMessage must include at least one recipient".to_string());
    }
    for recipient in recipients {
        let recipient = recipient
            .as_object()
            .ok_or_else(|| "encryptedMessage recipient must be an object".to_string())?;
        let key_id = recipient
            .get("keyId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "encryptedMessage recipient keyId is required".to_string())?;
        if key_id.len() > 512 {
            return Err("encryptedMessage recipient keyId is too long".to_string());
        }
        if required_encrypted_base64(recipient, "wrappedKey")?.is_empty() {
            return Err("encryptedMessage recipient wrappedKey must not be empty".to_string());
        }
    }
    Ok(())
}

fn validate_encrypted_media_payload(value: &Value) -> std::result::Result<(), String> {
    let payload = value
        .as_object()
        .ok_or_else(|| "encryptedMedia must be an object".to_string())?;
    match payload.get("v").and_then(Value::as_u64) {
        Some(1) => {}
        Some(version) => return Err(format!("unsupported encryptedMedia version {version}")),
        None => return Err("encryptedMedia.v is required".to_string()),
    }
    match payload.get("alg").and_then(Value::as_str) {
        Some("AES-256-GCM") => {}
        Some(_) => return Err("encryptedMedia.alg must be AES-256-GCM".to_string()),
        None => return Err("encryptedMedia.alg is required".to_string()),
    }
    let iv = required_encrypted_media_base64(payload, "iv")?;
    if iv.len() != 12 {
        return Err("encryptedMedia.iv must decode to 12 bytes".to_string());
    }
    if required_encrypted_media_base64(payload, "ciphertext")?.is_empty() {
        return Err("encryptedMedia.ciphertext must not be empty".to_string());
    }
    Ok(())
}

fn required_encrypted_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("encryptedMessage.{key} is required"))?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("encryptedMessage.{key} must be valid base64"))
}

fn required_encrypted_media_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("encryptedMedia.{key} is required"))?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("encryptedMedia.{key} must be valid base64"))
}

fn e2ee_device_fingerprint(credential: &str, key_package: &str) -> String {
    let digest = Sha256::digest(format!("{credential}\n{key_package}").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn peer_trust_state_after_material_update<'a>(
    existing_fingerprint: Option<&str>,
    existing_trust_state: Option<&'a str>,
    requested_trust_state: &'a str,
    new_fingerprint: &str,
) -> &'a str {
    if requested_trust_state == "trusted" {
        return "trusted";
    }
    if requested_trust_state == "revoked" {
        return "revoked";
    }
    match existing_fingerprint {
        Some(existing) if existing != new_fingerprint => "untrusted",
        Some(_) if existing_trust_state == Some("trusted") => "trusted",
        _ => requested_trust_state,
    }
}

fn insert_if_string(object: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        object.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn host_not_allowed(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".local")
        || host == "127.0.0.1"
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || private_172_host(host)
}

fn private_172_host(host: &str) -> bool {
    let Some(rest) = host.strip_prefix("172.") else {
        return false;
    };
    let Some(second) = rest
        .split('.')
        .next()
        .and_then(|part| part.parse::<u8>().ok())
    else {
        return false;
    };
    (16..=31).contains(&second)
}

fn stable_id(value: &str) -> String {
    let mut hash = 5381u32;
    for code in value.encode_utf16() {
        hash = hash.wrapping_shl(5).wrapping_add(hash) ^ u32::from(code);
    }
    base36(hash)
}

fn base36(mut value: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut chars = Vec::new();
    while value > 0 {
        let digit = (value % 36) as u8;
        chars.push(match digit {
            0..=9 => char::from(b'0' + digit),
            _ => char::from(b'a' + digit - 10),
        });
        value /= 36;
    }
    chars.into_iter().rev().collect()
}

fn clamp_limit(value: Option<String>) -> i32 {
    value
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(20)
        .clamp(1, 80)
}

fn string_field(row: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    row.and_then(|fields| fields.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn integer_field(row: Option<&Map<String, Value>>, key: &str) -> i64 {
    row.and_then(|fields| fields.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
                .or_else(|| value.as_f64().map(|number| number as i64))
                .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
        })
        .unwrap_or(0)
}

fn string_vec_json_field(row: Option<&Map<String, Value>>, key: &str) -> Vec<String> {
    let Some(raw) = string_field(row, key) else {
        return Vec::new();
    };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    items.iter().filter_map(optional_body_string).collect()
}

fn bool_field(row: Option<&Map<String, Value>>, key: &str) -> bool {
    row.and_then(|fields| fields.get(key))
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_i64().map(|number| number != 0))
                .or_else(|| value.as_u64().map(|number| number != 0))
                .or_else(|| {
                    value
                        .as_str()
                        .map(|text| matches!(text, "1" | "true" | "TRUE" | "yes" | "YES"))
                })
        })
        .unwrap_or(false)
}

fn row_value_or_null(row: &Map<String, Value>, key: &str) -> Value {
    non_empty_value(row, key).unwrap_or(Value::Null)
}

fn string_value_or_default(row: &Map<String, Value>, key: &str) -> Value {
    string_field(Some(row), key)
        .map(Value::String)
        .unwrap_or_else(|| Value::String(String::new()))
}

fn normalize_sensitive_categories(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let category = value.trim().to_ascii_lowercase();
        if !matches!(
            category.as_str(),
            "medical" | "adult" | "political" | "family-only" | "work-sensitive"
        ) {
            continue;
        }
        if !normalized.contains(&category) {
            normalized.push(category);
        }
    }
    normalized
}

fn row_value_or_fallback_null(row: &Map<String, Value>, key: &str, fallback: &str) -> Value {
    non_empty_value(row, key)
        .or_else(|| non_empty_value(row, fallback))
        .unwrap_or(Value::Null)
}

fn non_empty_value(row: &Map<String, Value>, key: &str) -> Option<Value> {
    row.get(key).and_then(|value| match value {
        Value::Null => None,
        Value::String(text) if text.is_empty() => None,
        _ => Some(value.clone()),
    })
}

fn require_owner_bearer(
    req: &Request,
    env: &Env,
    required_scopes: &[&str],
) -> Result<Option<Response>> {
    let tokens = owner_bearer_tokens(env);
    if tokens.is_empty() && remote_environment(env) {
        return Ok(Some(api_json(
            &serde_json::json!({ "error": "OWNER_API_TOKEN is not configured" }),
            503,
        )?));
    }
    let auth = req.headers().get("Authorization")?.unwrap_or_default();
    let provided = auth.strip_prefix("Bearer ").map(str::trim).unwrap_or("");
    let token = tokens.iter().find(|entry| entry.token == provided);
    match token {
        Some(entry) if owner_token_has_scopes(&entry.scopes, required_scopes) => Ok(None),
        Some(_) => Ok(Some(api_json(
            &serde_json::json!({
                "error": "Owner bearer token lacks required scope",
                "required_scopes": required_scopes,
            }),
            403,
        )?)),
        None => Ok(Some(api_json(
            &serde_json::json!({ "error": "Owner bearer token required" }),
            401,
        )?)),
    }
}

fn require_mastodon_bearer(req: &Request, env: &Env) -> Result<Option<Response>> {
    let configured = env
        .secret("OWNER_API_TOKEN")
        .map(|value| value.to_string())
        .or_else(|_| {
            env.secret("DAIS_OWNER_TOKEN")
                .map(|value| value.to_string())
        })
        .or_else(|_| env.var("OWNER_API_TOKEN").map(|value| value.to_string()))
        .or_else(|_| env.var("DAIS_OWNER_TOKEN").map(|value| value.to_string()))
        .unwrap_or_default();
    if configured.is_empty() && remote_environment(env) {
        return Ok(Some(api_json(
            &serde_json::json!({ "error": "OWNER_API_TOKEN is not configured" }),
            503,
        )?));
    }

    let expected = if configured.is_empty() {
        "dais-local-owner-token".to_string()
    } else {
        configured
    };
    let auth = req.headers().get("Authorization")?.unwrap_or_default();
    let provided = auth.strip_prefix("Bearer ").map(str::trim).unwrap_or("");
    if !provided.is_empty() && provided == expected {
        return Ok(None);
    }

    Ok(Some(api_json(
        &serde_json::json!({ "error": "Bearer token required" }),
        401,
    )?))
}

fn owner_bearer_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    let configured = env
        .secret("OWNER_API_TOKEN")
        .map(|value| value.to_string())
        .or_else(|_| {
            env.secret("DAIS_OWNER_TOKEN")
                .map(|value| value.to_string())
        })
        .or_else(|_| env.var("OWNER_API_TOKEN").map(|value| value.to_string()))
        .or_else(|_| env.var("DAIS_OWNER_TOKEN").map(|value| value.to_string()))
        .unwrap_or_else(|_| {
            if remote_environment(env) {
                String::new()
            } else {
                "dais-local-owner-token".to_string()
            }
        });
    if !configured.is_empty() {
        tokens.push(OwnerToken {
            token: configured,
            scopes: vec!["owner".to_string()],
        });
    }
    tokens.extend(scoped_owner_tokens(env));
    tokens
}

fn remote_environment(env: &Env) -> bool {
    env.var("ENVIRONMENT")
        .map(|value| value.to_string() != "dev")
        .unwrap_or(false)
}

fn scoped_owner_tokens(env: &Env) -> Vec<OwnerToken> {
    let raw = env
        .var("OWNER_API_SCOPED_TOKENS")
        .or_else(|_| env.var("DAIS_OWNER_SCOPED_TOKENS"))
        .map(|value| value.to_string())
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(token, scopes)| {
                let scopes = normalize_scopes(scopes);
                if token.trim().is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        Ok(Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| {
                let token = value
                    .get("token")
                    .or_else(|| value.get("value"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let scopes = normalize_scopes(
                    value
                        .get("scopes")
                        .or_else(|| value.get("scope"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                if token.is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scopes(value: Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(normalize_scope))
            .filter(|scope| !scope.is_empty())
            .collect(),
        Value::String(scopes) => scopes
            .split(|character: char| character == ',' || character.is_whitespace())
            .map(normalize_scope)
            .filter(|scope| !scope.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn owner_token_has_scopes(scopes: &[String], required_scopes: &[&str]) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "owner" || scope == "admin" || scope == "*")
        || required_scopes
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}

fn api_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set(
        "Access-Control-Allow-Headers",
        "Authorization, Content-Type",
    )?;
    headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, PATCH, DELETE, OPTIONS",
    )?;
    headers.set("Cache-Control", "no-store")?;
    headers.set("Vary", "Authorization, Accept")?;
    let mut response = if status == 204 {
        Response::empty()?.with_status(status)
    } else {
        Response::from_json(value)?.with_status(status)
    };
    response = response.with_headers(headers);
    Ok(response)
}

fn text_response(body: &str, content_type: &str) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", content_type)?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    Ok(Response::ok(body.to_string())?.with_headers(headers))
}

fn fixture_actor_response(url: &worker::Url) -> Result<Response> {
    let public_key = match fixture_public_key(url) {
        Some(value) => value,
        None => return Response::error("Missing or invalid fixture public key", 400),
    };
    let actor_url = url.to_string();
    let name = url
        .query_pairs()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "dais-s2s-fixture".to_string());
    activity_json(&FixtureActor {
        context: "https://www.w3.org/ns/activitystreams",
        id: &actor_url,
        actor_type: "Application",
        preferred_username: &name,
        name: &name,
        inbox: &format!(
            "{}://{}/__dais-fixtures/activitypub/inbox",
            url.scheme(),
            url.host_str().unwrap_or_default()
        ),
        outbox: &fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/outbox"),
        public_key: FixturePublicKey {
            id: &format!("{actor_url}#main-key"),
            owner: &actor_url,
            public_key_pem: &public_key,
        },
    })
}

fn fixture_outbox_response(url: &worker::Url) -> Result<Response> {
    let post = fixture_post(url);
    let create_id = format!("{}#create", post.id);
    activity_json(&FixtureOutbox {
        context: "https://www.w3.org/ns/activitystreams",
        id: &url.to_string(),
        collection_type: "OrderedCollection",
        total_items: 1,
        ordered_items: vec![FixtureCreate {
            id: &create_id,
            create_type: "Create",
            actor: post.attributed_to.clone(),
            to: post.to.clone(),
            object: post,
        }],
    })
}

fn fixture_post_response(url: &worker::Url) -> Result<Response> {
    activity_json(&fixture_post(url))
}

fn fixture_rss_response(url: &worker::Url) -> Result<Response> {
    let id = url
        .query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.to_string())
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        .unwrap_or_else(|| "source-fixture".to_string());
    let item_url = format!(
        "{}://{}/__dais-fixtures/sources/items/{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        id
    );
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Dais Source Fixture</title>
    <link>{item_url}</link>
    <description>Dais deterministic RSS fixture</description>
    <item>
      <title>Dais source fixture {id}</title>
      <link>{item_url}</link>
      <guid>{item_url}</guid>
      <author>fixtures@dais.social</author>
      <pubDate>Tue, 16 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[<p>Deterministic source refresh fixture for Dais parity smoke.</p>]]></description>
    </item>
  </channel>
</rss>
"#
    );
    let headers = Headers::new();
    headers.set("Content-Type", "application/rss+xml; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::ok(xml)?.with_headers(headers))
}

fn fixture_post(url: &worker::Url) -> FixturePost {
    let post_id =
        fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/posts/public-preview");
    FixturePost {
        context: "https://www.w3.org/ns/activitystreams",
        id: post_id.clone(),
        post_type: "Note",
        attributed_to: fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/actor"),
        to: vec![PUBLIC_COLLECTION.to_string()],
        content: "<p>Dais fixture public preview post</p>",
        published: "2026-06-16T00:00:00Z",
        url: post_id,
    }
}

fn activity_json<T: Serialize>(value: &T) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::from_json(value)?.with_headers(headers))
}

fn fixture_public_key(url: &worker::Url) -> Option<String> {
    let encoded = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())?;
    if encoded.len() > 2000
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    let base64 = encoded.replace('-', "+").replace('_', "/");
    let decoded = base64_decode(&base64)?;
    let pem = String::from_utf8(decoded).ok()?;
    if pem.contains("-----BEGIN PUBLIC KEY-----") && pem.contains("-----END PUBLIC KEY-----") {
        Some(pem)
    } else {
        None
    }
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for byte in value.bytes().filter(|byte| *byte != b'=') {
        let index = TABLE.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 6) | index;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Some(output)
}

fn fixture_url_with_public_key(url: &worker::Url, path: &str) -> String {
    let mut next = url.join(path).unwrap_or_else(|_| {
        worker::Url::parse(&format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            path
        ))
        .expect("fixture URL")
    });
    if let Some(public_key) = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())
    {
        next.query_pairs_mut().append_pair("pk", &public_key);
    }
    next.to_string()
}

#[derive(Serialize)]
struct FixtureActor<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    actor_type: &'a str,
    #[serde(rename = "preferredUsername")]
    preferred_username: &'a str,
    name: &'a str,
    inbox: &'a str,
    outbox: &'a str,
    #[serde(rename = "publicKey")]
    public_key: FixturePublicKey<'a>,
}

#[derive(Serialize)]
struct FixturePublicKey<'a> {
    id: &'a str,
    owner: &'a str,
    #[serde(rename = "publicKeyPem")]
    public_key_pem: &'a str,
}

#[derive(Clone, Serialize)]
struct FixturePost {
    #[serde(rename = "@context")]
    context: &'static str,
    id: String,
    #[serde(rename = "type")]
    post_type: &'static str,
    #[serde(rename = "attributedTo")]
    attributed_to: String,
    to: Vec<String>,
    content: &'static str,
    published: &'static str,
    url: String,
}

#[derive(Serialize)]
struct FixtureCreate<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    create_type: &'a str,
    actor: String,
    to: Vec<String>,
    object: FixturePost,
}

#[derive(Serialize)]
struct FixtureOutbox<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    collection_type: &'a str,
    #[serde(rename = "totalItems")]
    total_items: u8,
    #[serde(rename = "orderedItems")]
    ordered_items: Vec<FixtureCreate<'a>>,
}

#[derive(Serialize)]
struct OwnerProfile {
    id: String,
    username: String,
    actor_type: String,
    display_name: Option<String>,
    summary: Option<String>,
    icon: Option<String>,
    image: Option<String>,
    avatar_url: Option<String>,
    header_url: Option<String>,
    public_handle: String,
    actor_url: String,
}

#[derive(Serialize)]
struct OwnerStats {
    followers_total: i64,
    followers_approved: i64,
    followers_pending: i64,
    followers_rejected: i64,
    following_total: i64,
    posts_total: i64,
    activities_total: i64,
    deliveries_total: i64,
    deliveries_failed: i64,
    deliveries_queued: i64,
    deliveries_retry: i64,
    deliveries_delivered: i64,
    dual_protocol_posts: i64,
    public_posts: i64,
    private_posts: i64,
    direct_posts: i64,
    encrypted_posts: i64,
    media_posts: i64,
    notifications_unread: i64,
    blocks_total: i64,
    allowlist_hosts: i64,
    closed_network: bool,
}

#[derive(Serialize)]
struct OwnerItems<T> {
    items: Vec<T>,
}

#[derive(Serialize)]
struct OwnerSearch {
    posts: Vec<Map<String, Value>>,
    users: Vec<Map<String, Value>>,
    sources: Vec<Map<String, Value>>,
    source_items: Vec<Map<String, Value>>,
    public_posts: Vec<Map<String, Value>>,
    public_actors: Vec<Map<String, Value>>,
    provider_errors: Vec<Map<String, Value>>,
    public_search_guard: OwnerPublicSearchGuard,
}

#[derive(Default, Serialize)]
struct OwnerPublicSearchGuard {
    blocked: bool,
    requires_confirmation: bool,
    confirmed: bool,
    categories: Vec<String>,
    message: Option<String>,
}

#[derive(Serialize)]
struct OwnerSources {
    subscriptions: Vec<Map<String, Value>>,
    items: Vec<Map<String, Value>>,
}

#[derive(Serialize)]
struct OwnerModeration {
    closed_network: bool,
    block_count: i64,
    allowlist_count: i64,
    require_authorized_fetch: bool,
    manually_approves_followers: bool,
    reply_policy: String,
    ai_enabled: bool,
    ai_model: Option<String>,
    ai_daily_budget: i64,
    reply_queue_count: i64,
    flagged_reply_count: i64,
    hidden_reply_count: i64,
    rejected_reply_count: i64,
    blocks: Vec<Map<String, Value>>,
    allowlist: Vec<Map<String, Value>>,
}

struct ReplyModerationDecision {
    status: String,
    score: f64,
    flags: Vec<String>,
    hidden: bool,
}

#[derive(Serialize, Deserialize)]
struct WorkersAiModerationAdvisory {
    model: Option<String>,
    unsafe_detected: bool,
    categories: Vec<String>,
    summary: Option<String>,
}

#[derive(Serialize)]
struct OwnerDiagnostic {
    key: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Deserialize)]
struct DeliveryCount {
    status: String,
    count: i64,
}

struct LocalActor {
    id: String,
    private_key: String,
}

impl LocalActor {
    fn can_sign(&self) -> bool {
        !self.private_key.trim().is_empty()
    }
}

struct RemoteActor {
    id: String,
    actor_type: Option<String>,
    inbox: String,
    shared_inbox: Option<String>,
    preferred_username: Option<String>,
    name: Option<String>,
    summary: Option<String>,
    icon_url: Option<String>,
    url: Option<String>,
    outbox: Option<String>,
}

struct OwnerToken {
    token: String,
    scopes: Vec<String>,
}

#[cfg(test)]
#[cfg(test)]
mod tests;
