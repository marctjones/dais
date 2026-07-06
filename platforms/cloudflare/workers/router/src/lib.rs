use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dais_core::activitypub::sign_request;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use worker::{
    event, Context, D1Type, Env, Fetch, FormData, FormEntry, Headers, Request, RequestInit,
    Response, Result, ScheduleContext, ScheduledEvent,
};

mod activitypub;
mod audience;
mod config;
mod deliveries;
mod e2ee;
mod fixtures;
mod mastodon;
mod media;
mod moderation;
mod owner_auth;
mod public_search;
mod request;
mod response;
mod social;
mod sources;
#[cfg(test)]
pub(crate) use activitypub::display_local_url;
use activitypub::{
    accepts_activity_json, activitypub_actor_profile_html, activitypub_direct_to_actor,
    activitypub_note_object, activitypub_object_content_html, activitypub_public_recipients,
    actor_domain, collect_recipients, signature_actor_id, supported_timeline_object_type,
};
#[cfg(test)]
pub(crate) use audience::{
    audience_group_purpose_label, audience_membership_label, normalize_audience_group_type,
    normalize_audience_membership_visibility, normalize_audience_posting_policy,
};
use audience::{
    owner_audience_list_recipient_actors, owner_audience_lists, owner_delete_audience_list,
    owner_upsert_audience_list,
};
use config::{
    activitypub_domain, activitypub_user_prefix, handle_domain, local_actor_url,
    local_actor_url_for_request, local_username, origin, owner_instance_url,
};
use deliveries::{
    insert_delivery_rows, owner_deliveries, owner_delivery_action_path,
    owner_update_delivery_status,
};
#[cfg(test)]
pub(crate) use e2ee::{
    e2ee_device_fingerprint, normalize_e2ee_device_id, normalize_e2ee_fingerprint,
    normalize_e2ee_protocol, peer_trust_state_after_material_update, validate_e2ee_device_material,
    validate_encrypted_message_envelope, validate_owner_e2ee_payload,
};
pub(crate) use e2ee::{
    owner_delete_e2ee_message, owner_direct_messages, owner_discover_e2ee_peer_devices,
    owner_e2ee_devices, owner_e2ee_messages, owner_e2ee_peer_devices, owner_revoke_e2ee_device,
    owner_revoke_e2ee_peer_device, owner_send_e2ee_message, owner_trust_e2ee_peer_device,
    owner_upsert_e2ee_device, public_e2ee_devices, validate_dais_encrypted_message_v2,
    validate_encrypted_media_payload,
};
use fixtures::{
    fixture_actor_response, fixture_outbox_response, fixture_post_response, fixture_public_key,
    fixture_rss_response, fixture_url_with_public_key,
};
use mastodon::{
    account_action_path as mastodon_account_action_path,
    account_followers_path as mastodon_account_followers_path,
    account_following_path as mastodon_account_following_path,
    account_path as mastodon_account_path, account_statuses_path as mastodon_account_statuses_path,
    follow_request_action as mastodon_follow_request_action, media_path as mastodon_media_path,
    mentions as mastodon_mentions, notification_dismiss_path as mastodon_notification_dismiss_path,
    notification_type as mastodon_notification_type, parse_actor_acct, remote_account_json,
    status_action_path as mastodon_status_action_path, status_content as mastodon_status_content,
    status_context_path as mastodon_status_context_path, status_json as mastodon_status_json,
    status_path as mastodon_status_path, status_source_path as mastodon_status_source_path,
    suggestion_dismiss as mastodon_suggestion_dismiss, tags as mastodon_tags,
    visibility as mastodon_visibility,
};
#[cfg(test)]
pub(crate) use media::sha256_hex;
pub(crate) use media::{
    allowed_media_type, current_media_created_at, current_media_timestamp, handle_media,
    is_private_media_attachment, is_public_atproto_image_attachment, media_custom_metadata,
    media_r2_key_from_url, media_type_for_filename, private_media_expires_at, random_token,
    safe_media_filename, MediaMetadataInput,
};
#[cfg(test)]
pub(crate) use moderation::{
    normalize_ai_categories, parse_workers_ai_moderation, strip_json_fence,
};
use moderation::{
    owner_moderation, owner_moderation_replies, owner_set_reply_moderation_status,
    owner_update_moderation_settings,
};
#[cfg(test)]
pub(crate) use owner_auth::parse_scoped_owner_tokens;
use owner_auth::{owner_bearer_tokens, owner_token_has_scopes, remote_environment};
use public_search::{bluesky_appview_xrpc_url, bluesky_post_url, owner_search, owner_search_flags};
#[cfg(test)]
pub(crate) use public_search::{
    owner_normalize_bluesky_post, owner_normalize_tootfinder_status,
    owner_public_post_row_from_discovered, owner_public_search_mastodon_query_params,
    tootfinder_search_items, tootfinder_search_url, OwnerPublicSearchOptions,
};
#[cfg(test)]
pub(crate) use public_search::{OwnerPublicSearchProvider, OwnerPublicSearchResultType};
use request::{
    decode_component, optional_body_string, optional_trimmed_body, optional_url_field, query_param,
    read_json, read_mastodon_body, request_content_type, required_body_string, string_like_any,
    string_like_field,
};
use response::{activity_json, activitypub_error, api_json, jrd_json, text_response};
use social::{
    insert_block, owner_allow_host, owner_allowlist, owner_block, owner_blocks,
    owner_delete_allowlist_host, owner_discover_actor, owner_follow_actor, owner_followers,
    owner_following, owner_friends, owner_set_follower_status, owner_unblock, owner_unfollow_actor,
};
#[cfg(test)]
pub(crate) use sources::{
    activitypub_watch_item, bluesky_actor_target, bluesky_post_uri, bluesky_watch_item,
    normalized_source_target, source_id, source_policy_json_for_type, source_type_for_watch_kind,
    SourcePolicy,
};
use sources::{
    owner_add_source, owner_add_watch, owner_delete_source, owner_delete_watch,
    owner_refresh_sources, owner_refresh_watches, owner_source_items, owner_source_subscriptions,
    owner_watch_items, owner_watch_subscriptions, refresh_due_sources,
};

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";
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

pub(crate) async fn persist_mls_message_metadata(
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
    let media_attachments =
        normalize_owner_post_attachments(&attachments, encrypt, protocol, visibility)?;

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

pub(crate) async fn resolve_activitypub_actor_for_local(
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

pub(crate) async fn activitypub_actor_url_for_target(
    target: &str,
) -> std::result::Result<String, String> {
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

pub(crate) fn should_retry_signed_fetch(error: &str) -> bool {
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

pub(crate) async fn fetch_activitypub_json(
    url: &str,
    label: &str,
) -> std::result::Result<Value, String> {
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

pub(crate) async fn fetch_activitypub_json_signed(
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

pub(crate) async fn owner_local_actor(env: &Env) -> Result<LocalActor> {
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

fn normalize_owner_post_attachments(
    values: &[Value],
    encrypt: bool,
    protocol: &str,
    visibility: &str,
) -> std::result::Result<Vec<Value>, String> {
    let media_attachments = if encrypt {
        if matches!(protocol, "atproto" | "both") {
            return Err("encrypted media attachments are ActivityPub-only".to_string());
        }
        normalize_encrypted_media_attachments(values)?
    } else {
        normalize_attachments(values)?
    };
    if !media_attachments.is_empty()
        && matches!(protocol, "atproto" | "both")
        && !media_attachments
            .iter()
            .all(is_public_atproto_image_attachment)
    {
        return Err("AT Protocol media attachments must be public image uploads".to_string());
    }
    if !encrypt
        && !media_attachments.is_empty()
        && matches!(visibility, "followers" | "direct")
        && !media_attachments.iter().all(is_private_media_attachment)
    {
        return Err(
            "private and direct media attachments must use private media upload URLs".to_string(),
        );
    }
    Ok(media_attachments)
}

pub(crate) fn normalize_encrypted_media_attachments(
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

pub(crate) fn timestamp_for_local_id(iso: &str) -> String {
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

fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn clamp_cadence_minutes(value: Option<String>) -> i32 {
    let minutes = value
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(60.0);
    minutes.max(5.0).min(1440.0) as i32
}

pub(crate) fn body_string_any(body: &Value, keys: &[&str]) -> Option<String> {
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

pub(crate) fn public_https_url(value: &str, field: &str) -> std::result::Result<String, String> {
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

pub(crate) fn insert_if_string(object: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
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

pub(crate) fn stable_id(value: &str) -> String {
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

pub(crate) fn string_field(row: Option<&Map<String, Value>>, key: &str) -> Option<String> {
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

pub(crate) fn string_vec_json_field(row: Option<&Map<String, Value>>, key: &str) -> Vec<String> {
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

pub(crate) fn row_value_or_null(row: &Map<String, Value>, key: &str) -> Value {
    non_empty_value(row, key).unwrap_or(Value::Null)
}

pub(crate) fn string_value_or_default(row: &Map<String, Value>, key: &str) -> Value {
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
struct OwnerSources {
    subscriptions: Vec<Map<String, Value>>,
    items: Vec<Map<String, Value>>,
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

pub(crate) struct LocalActor {
    pub(crate) id: String,
    pub(crate) private_key: String,
}

impl LocalActor {
    pub(crate) fn can_sign(&self) -> bool {
        !self.private_key.trim().is_empty()
    }
}

pub(crate) struct RemoteActor {
    pub(crate) id: String,
    pub(crate) actor_type: Option<String>,
    pub(crate) inbox: String,
    pub(crate) shared_inbox: Option<String>,
    pub(crate) preferred_username: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) icon_url: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) outbox: Option<String>,
}

#[cfg(test)]
mod tests;
