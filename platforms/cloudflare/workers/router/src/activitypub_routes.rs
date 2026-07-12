use crate::activitypub::{
    accepts_activity_json, activitypub_actor_profile_html, activitypub_direct_to_actor,
    activitypub_note_object, activitypub_object_content_html, activitypub_public_recipients,
    actor_domain, signature_actor_id, supported_timeline_object_type,
};
use crate::config::{
    activitypub_domain, activitypub_user_prefix, handle_domain, local_actor_url,
    local_actor_url_for_request, local_username, origin,
};
use crate::e2ee::{public_e2ee_devices, validate_dais_encrypted_message_v2};
use crate::mastodon::{parse_actor_acct, status_content as mastodon_status_content};
use crate::mastodon_api::{mastodon_status_row, mastodon_status_rows, public_status_count};
use crate::media::media_type_for_filename;
use crate::owner::owner_profile;
use crate::request::{optional_body_string, query_param, read_json};
use crate::response::{activity_json, activitypub_error, api_json, jrd_json, text_response};
use crate::{
    canonical_mastodon_status_id, encrypted_media_attachments_from_activitypub_object,
    integer_field, is_local_object_url, owner_local_actor, resolve_activitypub_actor,
    resolve_activitypub_actor_for_local, row_value_or_null, stable_id, string_field, strip_html,
    value_string, PUBLIC_COLLECTION,
};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Headers, Request, Response, Result};

pub(crate) fn activitypub_public_path(env: &Env, path: &str) -> bool {
    let prefix = activitypub_user_prefix(env);
    path == prefix
        || path == format!("{prefix}/outbox")
        || path == format!("{prefix}/followers")
        || path == format!("{prefix}/following")
        || path == format!("{prefix}/followers_synchronization")
        || path == format!("{prefix}/inbox")
        || path.starts_with(&format!("{prefix}/posts/"))
}

pub(crate) async fn handle_activitypub_public(
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

pub(crate) fn activitypub_webfinger(env: &Env, url: &worker::Url) -> Result<Response> {
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
                        "v": 2,
                        "protocol": "mls-rfc9420",
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

pub(crate) async fn signed_approved_follower(env: &Env, req: &Request) -> Result<bool> {
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
