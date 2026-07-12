use serde::Serialize;
use serde_json::{Map, Value};
use worker::{event, Context, Env, Request, Response, Result, ScheduleContext, ScheduledEvent};

mod activitypub;
mod activitypub_routes;
mod audience;
mod config;
mod deliveries;
mod e2ee;
mod fixtures;
mod mastodon;
mod mastodon_api;
mod media;
mod moderation;
mod owner;
mod owner_auth;
mod posts;
mod public_search;
mod request;
mod response;
mod social;
mod sources;
#[cfg(test)]
pub(crate) use activitypub::activitypub_actor_profile_html;
#[cfg(test)]
pub(crate) use activitypub::display_local_url;
#[cfg(test)]
pub(crate) use activitypub::parse_lenient_json_body;
pub(crate) use activitypub::{
    activitypub_actor_url_for_target, actor_handle, discover_public_post_target,
    fetch_activitypub_json, fetch_activitypub_json_signed, fetch_actor_recent_public_posts,
    fetch_json_with_accept, fetch_lenient_json_with_accept, normalize_discovered_public_post,
    resolve_activitypub_actor, resolve_activitypub_actor_for_local,
    resolve_activitypub_object_inbox, should_retry_signed_fetch,
};
use activitypub_routes::{
    activitypub_public_path, activitypub_webfinger, handle_activitypub_public,
};
pub(crate) use activitypub_routes::{persist_mls_message_metadata, signed_approved_follower};
#[cfg(test)]
pub(crate) use audience::{
    audience_group_purpose_label, audience_membership_label, normalize_audience_group_type,
    normalize_audience_membership_visibility, normalize_audience_posting_policy,
};
use audience::{owner_audience_lists, owner_delete_audience_list, owner_upsert_audience_list};
use config::{activitypub_domain, local_actor_url, local_username, origin};
use deliveries::{owner_deliveries, owner_delivery_action_path, owner_update_delivery_status};
#[cfg(test)]
pub(crate) use e2ee::{
    e2ee_device_fingerprint, normalize_e2ee_device_id, normalize_e2ee_fingerprint,
    normalize_e2ee_protocol, peer_trust_state_after_material_update,
    validate_dais_encrypted_message_v2, validate_e2ee_device_material, validate_owner_e2ee_payload,
};
pub(crate) use e2ee::{
    owner_delete_e2ee_message, owner_direct_messages, owner_discover_e2ee_peer_devices,
    owner_e2ee_devices, owner_e2ee_messages, owner_e2ee_peer_devices, owner_revoke_e2ee_device,
    owner_revoke_e2ee_peer_device, owner_send_e2ee_message, owner_trust_e2ee_peer_device,
    owner_upsert_e2ee_device, validate_encrypted_media_payload,
};
use fixtures::{
    fixture_actor_response, fixture_outbox_response, fixture_post_response, fixture_rss_response,
};
pub(crate) use mastodon::status_content as mastodon_status_content;
use mastodon::{mentions as mastodon_mentions, tags as mastodon_tags};
use mastodon_api::{handle_mastodon_api, public_status_count};
#[cfg(test)]
pub(crate) use media::is_public_atproto_image_attachment;
#[cfg(test)]
pub(crate) use media::media_custom_metadata;
pub(crate) use media::media_type_for_filename;
#[cfg(test)]
pub(crate) use media::sha256_hex;
#[cfg(test)]
pub(crate) use media::MediaMetadataInput;
pub(crate) use media::{allowed_media_type, handle_media, owner_revoke_media, owner_upload_media};
#[cfg(test)]
pub(crate) use moderation::{
    normalize_ai_categories, parse_workers_ai_moderation, strip_json_fence,
};
use moderation::{
    owner_moderation, owner_moderation_replies, owner_set_reply_moderation_status,
    owner_update_moderation_settings,
};
pub(crate) use owner::OwnerProfile;
use owner::{
    owner_diagnostics, owner_profile, owner_settings, owner_snapshot_settings, owner_stats,
    owner_update_profile, owner_update_settings,
};
#[cfg(test)]
pub(crate) use owner_auth::parse_scoped_owner_tokens;
use owner_auth::{owner_bearer_tokens, owner_token_has_scopes, remote_environment};
#[cfg(test)]
pub(crate) use posts::normalize_owner_post_attachments;
use posts::{
    owner_create_post, owner_delete_post, owner_home_timeline, owner_mark_notification_read,
    owner_notifications, owner_post_detail, owner_posts, owner_publish_interaction,
    owner_save_post, owner_saved_posts, owner_unsave_post, shape_snapshot_home_timeline_item,
    shape_snapshot_post,
};
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
    decode_component, optional_body_string, query_param, read_json, required_body_string,
    string_like_any, string_like_field,
};
use response::api_json;
use social::{
    owner_allow_host, owner_allowlist, owner_block, owner_blocks, owner_delete_allowlist_host,
    owner_discover_actor, owner_federation_target_allowed, owner_follow_actor, owner_followers,
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

pub(crate) fn array_from_body_value(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(optional_body_string).collect(),
        Value::Null => Vec::new(),
        value => optional_body_string(value).into_iter().collect(),
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

pub(crate) fn clamp_limit(value: Option<String>) -> i32 {
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

#[derive(Serialize)]
struct OwnerItems<T> {
    items: Vec<T>,
}

#[derive(Serialize)]
struct OwnerSources {
    subscriptions: Vec<Map<String, Value>>,
    items: Vec<Map<String, Value>>,
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
