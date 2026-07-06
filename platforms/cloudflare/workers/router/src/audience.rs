use crate::{
    body_string_any, body_string_array_any, normalize_sensitive_categories, optional_body_string,
    row_value_or_null, stable_id, string_field, string_value_or_default, string_vec_json_field,
};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

pub(crate) fn normalize_audience_group_type(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "private_group" | "private" | "group" | "community" => "private_group",
        _ => "audience",
    }
}

pub(crate) fn normalize_audience_membership_visibility(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "members" | "member" | "group" => "members",
        "public" | "visible" => "public",
        _ => "private",
    }
}

pub(crate) fn normalize_audience_posting_policy(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "members" | "member" | "group" => "members",
        _ => "owner",
    }
}

pub(crate) fn audience_group_purpose_label(group_type: &str) -> &'static str {
    match normalize_audience_group_type(group_type) {
        "private_group" => "Private group",
        _ => "Audience list",
    }
}

pub(crate) fn audience_membership_label(membership_visibility: &str) -> &'static str {
    match normalize_audience_membership_visibility(membership_visibility) {
        "members" => "Membership visible to members",
        "public" => "Membership public",
        _ => "Membership private",
    }
}

pub(crate) async fn owner_audience_lists(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let lists = db
        .prepare(
            r#"
            SELECT id, name, description, allowed_categories, group_type,
                   membership_visibility, posting_policy, created_at, updated_at
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
        let group_type = normalize_audience_group_type(
            row.get("group_type")
                .and_then(Value::as_str)
                .unwrap_or("audience"),
        );
        let membership_visibility = normalize_audience_membership_visibility(
            row.get("membership_visibility")
                .and_then(Value::as_str)
                .unwrap_or("private"),
        );
        let posting_policy = normalize_audience_posting_policy(
            row.get("posting_policy")
                .and_then(Value::as_str)
                .unwrap_or("owner"),
        );
        item.insert(
            "group_type".to_string(),
            Value::String(group_type.to_string()),
        );
        item.insert(
            "membership_visibility".to_string(),
            Value::String(membership_visibility.to_string()),
        );
        item.insert(
            "posting_policy".to_string(),
            Value::String(posting_policy.to_string()),
        );
        item.insert(
            "purpose_label".to_string(),
            Value::String(audience_group_purpose_label(group_type).to_string()),
        );
        item.insert(
            "membership_label".to_string(),
            Value::String(audience_membership_label(membership_visibility).to_string()),
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

pub(crate) async fn owner_upsert_audience_list(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let name = body_string_any(body, &["name"]).ok_or_else(|| "name is required".to_string())?;
    let description = body.get("description").and_then(optional_body_string);
    let group_type = body_string_any(body, &["group_type", "groupType", "purpose"])
        .map(|value| normalize_audience_group_type(&value).to_string())
        .unwrap_or_else(|| "audience".to_string());
    let membership_visibility =
        body_string_any(body, &["membership_visibility", "membershipVisibility"])
            .map(|value| normalize_audience_membership_visibility(&value).to_string())
            .unwrap_or_else(|| "private".to_string());
    let posting_policy = body_string_any(body, &["posting_policy", "postingPolicy"])
        .map(|value| normalize_audience_posting_policy(&value).to_string())
        .unwrap_or_else(|| "owner".to_string());
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
    let group_type_arg = D1Type::Text(group_type.as_str());
    let membership_visibility_arg = D1Type::Text(membership_visibility.as_str());
    let posting_policy_arg = D1Type::Text(posting_policy.as_str());
    let allowed_categories_json =
        serde_json::to_string(&allowed_categories).map_err(|error| error.to_string())?;
    let categories_arg = D1Type::Text(allowed_categories_json.as_str());

    db.prepare(
        r#"
        INSERT INTO audience_lists (
          id, name, description, allowed_categories, group_type,
          membership_visibility, posting_policy, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          description = excluded.description,
          allowed_categories = excluded.allowed_categories,
          group_type = excluded.group_type,
          membership_visibility = excluded.membership_visibility,
          posting_policy = excluded.posting_policy,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([
        &id_arg,
        &name_arg,
        &description_arg,
        &categories_arg,
        &group_type_arg,
        &membership_visibility_arg,
        &posting_policy_arg,
    ])
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

pub(crate) async fn owner_delete_audience_list(env: &Env, id: &str) -> Result<()> {
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

pub(crate) async fn owner_audience_list_recipient_actors(
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
