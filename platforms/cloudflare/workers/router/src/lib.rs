use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use wasm_bindgen::{JsCast, JsValue};
use worker::{event, Context, D1Type, Env, Fetch, Headers, Request, RequestInit, Response, Result};

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();
    let host = url.host_str().unwrap_or_default();

    if host == "social.dais.social" && path == "/" {
        let target = url.join("/users/social")?;
        return Response::redirect(target);
    }

    if path.starts_with("/api/dais/owner/") {
        return handle_owner_api(req, env, &url).await;
    }

    match path {
        "/__dais-fixtures/activitypub/actor" => fixture_actor_response(&url),
        "/__dais-fixtures/activitypub/outbox" => fixture_outbox_response(&url),
        "/__dais-fixtures/activitypub/posts/public-preview" => fixture_post_response(&url),
        "/health" => Response::ok("OK"),
        _ => Response::error(
            "Rust router migration scaffold: route not migrated yet",
            501,
        ),
    }
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
            if visibility == "direct" && recipients.is_empty() {
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
        (worker::Method::Get, "/direct-messages") => api_json(
            &OwnerItems {
                items: owner_direct_messages(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/search") => api_json(
            &owner_search(&env, query_param(url, "q").unwrap_or_default(), limit).await?,
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
        worker::Method::Get => &["read"],
        worker::Method::Delete => &["write"],
        _ if path == "/discovery/actor" => &["read"],
        _ if path == "/followers/status"
            || path == "/following/follow"
            || path == "/following/unfollow" =>
        {
            &["follow"]
        }
        _ if path.starts_with("/moderation/") => &["moderation"],
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
    let actor_url = string_field(row.as_ref(), "id")
        .unwrap_or_else(|| "https://social.dais.social/users/social".to_string());
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

    let mut custom_metadata = HashMap::new();
    let description = body.get("description").and_then(optional_body_string);
    if let Some(description) = description.as_deref() {
        custom_metadata.insert("description".to_string(), description.to_string());
    }
    if let Some(expires_at) = expires_at.as_deref() {
        custom_metadata.insert("expires_at".to_string(), expires_at.to_string());
    }
    if require_authorized_fetch {
        custom_metadata.insert("authorized_fetch".to_string(), "required".to_string());
    }

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
            "https://social.dais.social/media/{}/{}/{}",
            if require_authorized_fetch {
                "_private_signed"
            } else {
                "_private"
            },
            token,
            safe_name
        )
    } else {
        format!("https://social.dais.social/media/{key}")
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
    attachment.insert("name".to_string(), Value::String(safe_name));

    let mut response = Map::new();
    response.insert("url".to_string(), Value::String(url));
    response.insert("media_type".to_string(), Value::String(media_type));
    response.insert("access".to_string(), Value::String(access));
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
    Ok(db
        .prepare(
            r#"
            SELECT default_visibility, require_authorized_fetch, manually_approves_followers,
                   COALESCE(closed_network, 0) AS closed_network
            FROM instance_settings
            WHERE id = 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?
        .unwrap_or_else(|| {
            let mut settings = Map::new();
            settings.insert(
                "default_visibility".to_string(),
                Value::String("followers".to_string()),
            );
            settings.insert("require_authorized_fetch".to_string(), Value::from(1));
            settings.insert("manually_approves_followers".to_string(), Value::from(1));
            settings.insert("closed_network".to_string(), Value::from(0));
            settings
        }))
}

async fn owner_snapshot(env: &Env) -> Result<Map<String, Value>> {
    let profile = owner_profile(env).await?;
    let home_timeline = owner_home_timeline(env, 20, false).await?;
    let posts = owner_posts(env, 20).await?;
    let followers = owner_followers(env, 100).await?;
    let friends = owner_friends(env, 100).await?;
    let following = owner_following(env, 100).await?;
    let sources = owner_source_items(env, 20).await?;
    let moderation = owner_moderation(env).await?;
    let diagnostics = owner_diagnostics(env).await?;
    let settings = owner_settings(env).await?;

    let mut snapshot_settings = Map::new();
    snapshot_settings.insert(
        "instance_url".to_string(),
        Value::String("https://social.dais.social".to_string()),
    );
    snapshot_settings.insert("owner_token_present".to_string(), Value::Bool(true));
    let default_visibility = string_field(Some(&settings), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    snapshot_settings.insert(
        "default_visibility".to_string(),
        Value::String(title_visibility(Some(default_visibility.as_str()))),
    );
    snapshot_settings.insert(
        "default_protocol".to_string(),
        Value::String("Both".to_string()),
    );

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
    let settings = owner_settings(env).await?;
    let blocks = db
        .prepare("SELECT COUNT(*) AS count FROM blocks")
        .first::<Map<String, Value>>(None)
        .await?;
    let allowlist = db
        .prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1")
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerModeration {
        closed_network: bool_field(Some(&settings), "closed_network"),
        block_count: integer_field(blocks.as_ref(), "count"),
        allowlist_count: integer_field(allowlist.as_ref(), "count"),
        require_authorized_fetch: bool_field(Some(&settings), "require_authorized_fetch"),
        manually_approves_followers: bool_field(Some(&settings), "manually_approves_followers"),
        blocks: owner_blocks(env).await?,
        allowlist: owner_allowlist(env).await?,
    })
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

async fn owner_post_detail(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
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
    let replies = owner_post_replies(env, id).await?;
    let likes = owner_post_interactions(env, id, "like").await?;
    let boosts = owner_post_interactions(env, id, "boost").await?;
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
) -> std::result::Result<Map<String, Value>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let actor = db
        .prepare("SELECT id FROM actors WHERE username = 'social' LIMIT 1")
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    let actor_id = string_field(actor.as_ref(), "id")
        .unwrap_or_else(|| "https://social.dais.social/users/social".to_string());
    let direct_targets = if visibility == "direct" {
        owner_direct_delivery_targets(env, &recipients).await?
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
    if !media_attachments.is_empty() && protocol != "activitypub" {
        return Err("media attachments currently require ActivityPub routing; AT Protocol media upload is not implemented yet".to_string());
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
        if !is_local_object_url(in_reply_to) {
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
    let object_type_arg = D1Type::Text("Note");
    let summary_arg = D1Type::Null;
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
    let poll_arg = D1Type::Null;
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
    response.insert("object_type".to_string(), Value::String("Note".to_string()));
    response.insert("summary".to_string(), Value::Null);
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
    response.insert("poll_options".to_string(), Value::Null);
    response.insert(
        "recipients".to_string(),
        Value::Array(recipients.into_iter().map(Value::String).collect()),
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
               content, published_at, created_at
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
        SELECT id, type, actor_id, actor_username, actor_display_name, actor_avatar_url,
               post_id, activity_id, content, read, created_at
        FROM notifications
        ORDER BY created_at DESC
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

async fn owner_search(env: &Env, query: String, limit: i32) -> Result<OwnerSearch> {
    let term = query.trim().to_string();
    if term.is_empty() {
        return Ok(OwnerSearch {
            posts: Vec::new(),
            users: Vec::new(),
            sources: Vec::new(),
            source_items: Vec::new(),
        });
    }

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

    Ok(OwnerSearch {
        posts,
        users,
        sources,
        source_items,
    })
}

async fn owner_source_subscriptions(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
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
    let source_type = string_like_any(body, &["source_type", "sourceType"])
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(source_type.as_str(), "rss" | "atom" | "api") {
        return Err("source_type must be rss, atom, or api".to_string());
    }
    let source_url = public_https_url(
        &string_like_field(body, "url").unwrap_or_default(),
        "source url",
    )?;
    let id = source_id(&source_type, &source_url);
    let title = body.get("title").and_then(optional_body_string);
    let cadence_minutes = clamp_cadence_minutes(string_like_any(
        body,
        &["cadence_minutes", "cadenceMinutes"],
    ));
    let api_secret_name = string_like_any(body, &["api_secret_name", "apiSecretName"])
        .and_then(|value| optional_body_string(&Value::String(value)));
    let policy_json = source_policy_json(body);

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&id);
    let type_arg = D1Type::Text(&source_type);
    let url_arg = D1Type::Text(&source_url);
    let title_arg = title.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
    let cadence_arg = D1Type::Integer(cadence_minutes);
    let policy_arg = D1Type::Text(&policy_json);
    let secret_arg = api_secret_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
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

async fn owner_delete_source(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("DELETE FROM source_subscriptions WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
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
    let object_id = public_https_url(object_id, "object_id")?;
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
    let target_inbox = resolve_activitypub_object_inbox(&object_id).await?;
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
                "object": object_id,
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
            "object": object_id,
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
    let remote = resolve_activitypub_actor(target).await?;
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
    let remote = resolve_activitypub_actor(&actor_target).await?;
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
    let remote = resolve_activitypub_actor(target).await?;
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
    let actor_url = if target.starts_with("http://") || target.starts_with("https://") {
        public_https_url(target, "target")?
    } else {
        resolve_webfinger_actor(target).await?
    };
    let actor = fetch_activitypub_json(&actor_url, "actor").await?;
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
    fetch_json_with_accept(
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
        label,
    )
    .await
}

async fn fetch_json_with_accept(
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
    response
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())
}

fn local_object_inbox(object_id: &str) -> Option<String> {
    let url = worker::Url::parse(object_id).ok()?;
    if url.host_str()? != "social.dais.social" {
        return None;
    }
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
    if !matches!(object_type, "Note" | "Question" | "Article") {
        return None;
    }
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
    let content = object
        .get("content")
        .or_else(|| object.get("name"))
        .or_else(|| object.get("summary"))
        .and_then(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| optional_body_string(value))
        })
        .unwrap_or_default();
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
        .prepare("SELECT id, username FROM actors WHERE username = 'social' LIMIT 1")
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(LocalActor {
        id: string_field(row.as_ref(), "id")
            .unwrap_or_else(|| "https://social.dais.social/users/social".to_string()),
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
    if parsed.host_str()? != "social.dais.social" {
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
            url.host_str() == Some("social.dais.social")
                && (url.path().starts_with("/media/_private/")
                    || url.path().starts_with("/media/_private_signed/"))
        })
        .unwrap_or(false)
}

fn is_local_object_url(value: &str) -> bool {
    worker::Url::parse(value)
        .ok()
        .map(|url| {
            url.host_str() == Some("social.dais.social") && url.path().starts_with("/users/social/")
        })
        .unwrap_or(false)
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

fn source_policy_json(body: &Value) -> String {
    format!(
        "{{\"private_reader_only\":{},\"excerpt_only\":{},\"link_required\":{},\"attribution_required\":{},\"image_allowed\":{},\"full_text_allowed\":{}}}",
        source_policy_default_true(body, "private_reader_only", "privateReaderOnly"),
        source_policy_default_true(body, "excerpt_only", "excerptOnly"),
        source_policy_default_true(body, "link_required", "linkRequired"),
        source_policy_default_true(body, "attribution_required", "attributionRequired"),
        source_policy_bool(body, "image_allowed", "imageAllowed"),
        source_policy_bool(body, "full_text_allowed", "fullTextAllowed"),
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

fn source_id(source_type: &str, source_url: &str) -> String {
    let digest = Sha256::digest(format!("{source_type}\n{source_url}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("source-{}", &hex[..24])
}

fn body_string_any(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| required_body_string(body.get(*key)))
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
    if tokens.is_empty()
        && env
            .var("ENVIRONMENT")
            .map(|value| value.to_string() == "production")
            .unwrap_or(false)
    {
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

fn owner_bearer_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    let configured = env
        .var("OWNER_API_TOKEN")
        .or_else(|_| env.var("DAIS_OWNER_TOKEN"))
        .map(|value| value.to_string())
        .unwrap_or_else(|_| {
            if env
                .var("ENVIRONMENT")
                .map(|value| value.to_string() == "production")
                .unwrap_or(false)
            {
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
    let mut response = if status == 204 {
        Response::empty()?.with_status(status)
    } else {
        Response::from_json(value)?.with_status(status)
    };
    response = response.with_headers(headers);
    Ok(response)
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
    blocks: Vec<Map<String, Value>>,
    allowlist: Vec<Map<String, Value>>,
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
