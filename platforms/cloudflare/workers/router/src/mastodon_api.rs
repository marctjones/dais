use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::config::activitypub_domain;
use crate::mastodon::{
    account_action_path as mastodon_account_action_path,
    account_followers_path as mastodon_account_followers_path,
    account_following_path as mastodon_account_following_path,
    account_path as mastodon_account_path, account_statuses_path as mastodon_account_statuses_path,
    follow_request_action as mastodon_follow_request_action, media_path as mastodon_media_path,
    notification_dismiss_path as mastodon_notification_dismiss_path,
    notification_type as mastodon_notification_type, remote_account_json,
    status_action_path as mastodon_status_action_path, status_content as mastodon_status_content,
    status_context_path as mastodon_status_context_path, status_json as mastodon_status_json,
    status_path as mastodon_status_path, status_source_path as mastodon_status_source_path,
    suggestion_dismiss as mastodon_suggestion_dismiss, visibility as mastodon_visibility,
};
use crate::media::{is_private_media_attachment, media_type_for_filename, owner_upload_media};
use crate::owner::{owner_settings, owner_update_profile};
use crate::owner_auth::{owner_bearer_tokens, owner_token_has_scopes, remote_environment};
use crate::posts::{owner_create_post, owner_delete_post};
use crate::request::{
    decode_component, optional_body_string, query_param, read_mastodon_body, request_content_type,
};
use crate::response::{api_json, text_response};
use crate::social::{insert_block, owner_discover_actor, owner_follow_actor, owner_unfollow_actor};
use crate::{
    array_from_body_value, body_string_any, canonical_mastodon_status_id, clamp_limit, escape_html,
    integer_field, normalize_host_value, owner_local_actor, row_value_or_null, stable_id,
    string_field,
};
use serde_json::{Map, Value};
use worker::{D1Type, Env, FormData, FormEntry, Headers, Request, Response, Result};

pub(crate) async fn handle_mastodon_api(
    mut req: Request,
    env: Env,
    url: &worker::Url,
) -> Result<Response> {
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
        (worker::Method::Get, "/api/v1/mutes") => {
            if let Some(response) = require_mastodon_bearer(&req, &env)? {
                return Ok(response);
            }
            api_json(
                &mastodon_mutes(&env, clamp_limit(query_param(url, "limit"))).await?,
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
            &serde_json::json!({
                "error": "unsupported Mastodon compatibility endpoint",
                "detail": "Dais supports a scoped Mastodon-compatible client API, not the full Mastodon API surface. Track full owner-auth separation in issue #333."
            }),
            404,
        ),
    }
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
            "dais_authentication": "owner_token_required",
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

pub(crate) async fn mastodon_status_rows(
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

pub(crate) async fn mastodon_status_row(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
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
        "media_type": media_type,
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

async fn mastodon_mutes(env: &Env, limit: i32) -> Result<Value> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT actor_id, actor_id AS url, created_at
            FROM mutes
            WHERE actor_id IS NOT NULL AND actor_id != ''
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
        "mute" => {
            mastodon_set_account_mute(env, id, true).await?;
        }
        "unmute" => {
            mastodon_set_account_mute(env, id, false).await?;
        }
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

async fn mastodon_set_account_mute(env: &Env, actor_id: &str, enabled: bool) -> Result<()> {
    let db = env.d1("DB")?;
    let actor_arg = D1Type::Text(actor_id);
    if enabled {
        let id = format!("mute-{}", stable_id(actor_id));
        let id_arg = D1Type::Text(&id);
        db.prepare(
            r#"
            INSERT OR REPLACE INTO mutes (id, actor_id, reason, created_at)
            VALUES (?1, ?2, 'Mastodon API mute', CURRENT_TIMESTAMP)
            "#,
        )
        .bind_refs([&id_arg, &actor_arg])?
        .run()
        .await?;
    } else {
        db.prepare("DELETE FROM mutes WHERE actor_id = ?1")
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
    let muted = db
        .prepare("SELECT 1 FROM mutes WHERE actor_id = ?1 LIMIT 1")
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
        "muting": muted,
        "muting_notifications": muted,
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

pub(crate) async fn public_status_count(env: &Env) -> Result<i64> {
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

fn require_mastodon_bearer(req: &Request, env: &Env) -> Result<Option<Response>> {
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
    if matches!(
        token,
        Some(entry) if owner_token_has_scopes(&entry.scopes, &["owner"])
    ) {
        return Ok(None);
    }

    if token.is_some() {
        return Ok(Some(api_json(
            &serde_json::json!({ "error": "Bearer token lacks required scope" }),
            403,
        )?));
    }

    Ok(Some(api_json(
        &serde_json::json!({ "error": "Bearer token required" }),
        401,
    )?))
}
