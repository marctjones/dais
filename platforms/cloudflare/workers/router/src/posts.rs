use crate::audience::owner_audience_list_recipient_actors;
use crate::config::{activitypub_domain, local_actor_url};
use crate::deliveries::insert_delivery_rows;
use crate::media::{
    allowed_media_type, is_private_media_attachment, is_public_atproto_image_attachment,
};
use crate::request::{optional_body_string, optional_trimmed_body};
use crate::social::owner_approved_follower_inboxes;
use crate::{
    canonical_mastodon_status_id, escape_html, integer_field, is_local_object_url, non_empty_value,
    normalize_encrypted_media_attachments, owner_local_actor, parse_attachment_array,
    public_https_url, resolve_activitypub_object_inbox, row_value_or_null, stable_id, string_field,
    string_value_or_default, timestamp_for_local_id, title_protocol, title_visibility,
    PUBLIC_COLLECTION,
};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

pub(crate) fn shape_snapshot_home_timeline_item(post: Map<String, Value>) -> Map<String, Value> {
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

pub(crate) fn shape_snapshot_post(post: Map<String, Value>) -> Map<String, Value> {
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

pub(crate) async fn owner_posts(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_saved_posts(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_save_post(
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

pub(crate) async fn owner_unsave_post(env: &Env, id: &str) -> Result<()> {
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

pub(crate) async fn owner_post_detail(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
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

pub(crate) async fn owner_delete_post(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
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

pub(crate) async fn owner_create_post(
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

pub(crate) async fn owner_home_timeline(
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

pub(crate) async fn owner_notifications(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_mark_notification_read(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("UPDATE notifications SET read = 1 WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

pub(crate) async fn owner_publish_interaction(
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

pub(crate) fn normalize_owner_post_attachments(
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
