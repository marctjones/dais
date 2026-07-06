use crate::{decode_component, owner_federation_target_allowed, stable_id, string_field};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

pub(crate) async fn owner_deliveries(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) fn owner_delivery_action_path(path: &str) -> Option<(String, &'static str)> {
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

pub(crate) async fn owner_delivery_rows_for_post(
    env: &Env,
    post_id: &str,
) -> Result<Vec<Map<String, Value>>> {
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

pub(crate) async fn owner_update_delivery_status(
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

pub(crate) async fn insert_delivery_rows(
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
