use crate::request::optional_body_string;
use crate::{
    body_string_any, normalize_host_value, owner_local_actor, public_https_url, stable_id,
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
