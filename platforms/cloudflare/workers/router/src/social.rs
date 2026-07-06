use crate::deliveries::insert_delivery_rows;
use crate::request::optional_body_string;
use crate::{
    actor_handle, body_string_any, discover_public_post_target, fetch_actor_recent_public_posts,
    normalize_host_value, owner_local_actor, public_https_url, resolve_activitypub_actor_for_local,
    stable_id, string_field,
};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

pub(crate) async fn owner_blocks(env: &Env) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_allowlist(env: &Env) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_followers(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_friends(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_following(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_unblock(env: &Env, value: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let value_arg = D1Type::Text(value);
    db.prepare("DELETE FROM blocks WHERE id = ?1 OR actor_id = ?1 OR blocked_domain = ?1")
        .bind_refs(&value_arg)?
        .run()
        .await?;
    Ok(())
}

pub(crate) async fn owner_block(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
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

pub(crate) async fn insert_block(
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

pub(crate) async fn owner_delete_allowlist_host(env: &Env, host: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let host_arg = D1Type::Text(host);
    db.prepare("DELETE FROM federation_allowlist WHERE host = ?1")
        .bind_refs(&host_arg)?
        .run()
        .await?;
    Ok(())
}

pub(crate) async fn owner_allow_host(
    env: &Env,
    host: &str,
    note: Option<&str>,
) -> Result<Map<String, Value>> {
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

pub(crate) async fn owner_follow_actor(
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

pub(crate) async fn owner_discover_actor(
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

pub(crate) async fn owner_unfollow_actor(
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

pub(crate) async fn owner_set_follower_status(
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
