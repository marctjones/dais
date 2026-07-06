use crate::{
    body_string_any, bool_field, integer_field, optional_body_string, owner_allowlist,
    owner_blocks, owner_settings, string_field, string_vec_json_field,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

#[derive(Serialize)]
pub(crate) struct OwnerModeration {
    closed_network: bool,
    block_count: i64,
    allowlist_count: i64,
    require_authorized_fetch: bool,
    manually_approves_followers: bool,
    reply_policy: String,
    ai_enabled: bool,
    ai_model: Option<String>,
    ai_daily_budget: i64,
    reply_queue_count: i64,
    flagged_reply_count: i64,
    hidden_reply_count: i64,
    rejected_reply_count: i64,
    blocks: Vec<Map<String, Value>>,
    allowlist: Vec<Map<String, Value>>,
}

struct ReplyModerationDecision {
    status: String,
    score: f64,
    flags: Vec<String>,
    hidden: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct WorkersAiModerationAdvisory {
    pub(crate) model: Option<String>,
    pub(crate) unsafe_detected: bool,
    pub(crate) categories: Vec<String>,
    pub(crate) summary: Option<String>,
}

pub(crate) async fn owner_moderation(env: &Env) -> Result<OwnerModeration> {
    let db = env.d1("DB")?;
    owner_refresh_reply_moderation(env, 120).await?;
    let settings = owner_settings(env).await?;
    let moderation_settings = owner_moderation_settings(env).await?;
    let blocks = db
        .prepare("SELECT COUNT(*) AS count FROM blocks")
        .first::<Map<String, Value>>(None)
        .await?;
    let allowlist = db
        .prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1")
        .first::<Map<String, Value>>(None)
        .await?;
    let reply_counts = db
        .prepare(
            r#"
            SELECT
                COUNT(*) AS total_count,
                SUM(CASE WHEN moderation_status = 'pending' THEN 1 ELSE 0 END) AS pending_count,
                SUM(CASE WHEN moderation_status = 'hidden' THEN 1 ELSE 0 END) AS hidden_count,
                SUM(CASE WHEN moderation_status = 'rejected' THEN 1 ELSE 0 END) AS rejected_count,
                SUM(
                    CASE
                        WHEN moderation_flags IS NOT NULL
                         AND moderation_flags != ''
                         AND moderation_flags != '[]'
                        THEN 1
                        ELSE 0
                    END
                ) AS flagged_count
            FROM replies
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerModeration {
        closed_network: bool_field(Some(&settings), "closed_network"),
        block_count: integer_field(blocks.as_ref(), "count"),
        allowlist_count: integer_field(allowlist.as_ref(), "count"),
        require_authorized_fetch: bool_field(Some(&settings), "require_authorized_fetch"),
        manually_approves_followers: bool_field(Some(&settings), "manually_approves_followers"),
        reply_policy: string_field(moderation_settings.as_ref(), "reply_policy")
            .unwrap_or_else(|| "warn".to_string()),
        ai_enabled: bool_field(moderation_settings.as_ref(), "ai_enabled"),
        ai_model: string_field(moderation_settings.as_ref(), "ai_model"),
        ai_daily_budget: integer_field(moderation_settings.as_ref(), "ai_daily_budget"),
        reply_queue_count: integer_field(reply_counts.as_ref(), "pending_count"),
        flagged_reply_count: integer_field(reply_counts.as_ref(), "flagged_count"),
        hidden_reply_count: integer_field(reply_counts.as_ref(), "hidden_count"),
        rejected_reply_count: integer_field(reply_counts.as_ref(), "rejected_count"),
        blocks: owner_blocks(env).await?,
        allowlist: owner_allowlist(env).await?,
    })
}

async fn owner_moderation_settings(env: &Env) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, reply_policy, ai_enabled, ai_model, ai_daily_budget
        FROM moderation_settings
        WHERE id = 1
        LIMIT 1
        "#,
    )
    .first::<Map<String, Value>>(None)
    .await
}

pub(crate) async fn owner_moderation_replies(
    env: &Env,
    limit: i32,
) -> Result<Vec<Map<String, Value>>> {
    owner_refresh_reply_moderation(env, limit).await?;
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, post_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
                   content, published_at, created_at, moderation_status, moderation_score,
                   moderation_flags, moderation_checked_at, ai_moderation_result, hidden
            FROM replies
            WHERE moderation_status != 'approved'
               OR (hidden IS NOT NULL AND hidden != 0)
               OR (moderation_flags IS NOT NULL AND moderation_flags != '' AND moderation_flags != '[]')
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(shape_owner_moderation_reply).collect())
}

pub(crate) async fn owner_set_reply_moderation_status(
    env: &Env,
    reply_id: &str,
    status: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let normalized = normalize_reply_moderation_status(status)?;
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let reply_arg = D1Type::Text(reply_id);
    let status_arg = D1Type::Text(&normalized);
    let hidden_arg = D1Type::Integer(if normalized == "approved" { 0 } else { 1 });
    db.prepare(
        r#"
        UPDATE replies
        SET moderation_status = ?2,
            hidden = ?3,
            moderation_checked_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
    )
    .bind_refs([&reply_arg, &status_arg, &hidden_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_moderation_reply(env, reply_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "reply not found".to_string())
}

pub(crate) async fn owner_update_moderation_settings(
    env: &Env,
    body: &Value,
) -> std::result::Result<OwnerModeration, String> {
    let reply_policy = normalize_reply_policy(
        body_string_any(body, &["reply_policy", "replyPolicy"])
            .unwrap_or_else(|| "warn".to_string())
            .as_str(),
    )?
    .to_string();
    let ai_enabled = body
        .get("ai_enabled")
        .or_else(|| body.get("aiEnabled"))
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| optional_body_string(value).map(|v| v == "true" || v == "1"))
        })
        .unwrap_or(false);
    let ai_model = body
        .get("ai_model")
        .or_else(|| body.get("aiModel"))
        .and_then(optional_body_string);
    let ai_daily_budget = body
        .get("ai_daily_budget")
        .or_else(|| body.get("aiDailyBudget"))
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(0);
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let policy_arg = D1Type::Text(&reply_policy);
    let ai_enabled_arg = D1Type::Integer(if ai_enabled { 1 } else { 0 });
    let ai_model_arg = ai_model
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let ai_daily_budget_i32 = i32::try_from(ai_daily_budget).unwrap_or(i32::MAX);
    let ai_budget_arg = D1Type::Integer(ai_daily_budget_i32);
    db.prepare(
        r#"
        INSERT INTO moderation_settings (
            id, reply_policy, ai_enabled, ai_model, ai_daily_budget, updated_at
        ) VALUES (1, ?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            reply_policy = excluded.reply_policy,
            ai_enabled = excluded.ai_enabled,
            ai_model = excluded.ai_model,
            ai_daily_budget = excluded.ai_daily_budget,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([&policy_arg, &ai_enabled_arg, &ai_model_arg, &ai_budget_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_reclassify_recent_replies(env, 120)
        .await
        .map_err(|error| error.to_string())?;
    owner_moderation(env)
        .await
        .map_err(|error| error.to_string())
}

async fn owner_reclassify_recent_replies(env: &Env, limit: i32) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, content
            FROM replies
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    for row in rows {
        let Some(reply_id) = string_field(Some(&row), "id") else {
            continue;
        };
        let content = string_field(Some(&row), "content").unwrap_or_default();
        classify_reply_in_db(env, &reply_id, &content).await?;
    }
    Ok(())
}

async fn owner_refresh_reply_moderation(env: &Env, limit: i32) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, content
            FROM replies
            WHERE moderation_checked_at IS NULL
            ORDER BY published_at DESC, created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    for row in rows {
        let Some(reply_id) = string_field(Some(&row), "id") else {
            continue;
        };
        let content = string_field(Some(&row), "content").unwrap_or_default();
        classify_reply_in_db(env, &reply_id, &content).await?;
    }
    Ok(())
}

async fn classify_reply_in_db(
    env: &Env,
    reply_id: &str,
    content: &str,
) -> std::result::Result<(), String> {
    let settings = owner_moderation_settings(env)
        .await
        .map_err(|error| error.to_string())?;
    let policy = settings
        .as_ref()
        .and_then(|row| string_field(Some(row), "reply_policy"))
        .unwrap_or_else(|| "warn".to_string());
    let mut result = classify_reply_content(content, &policy)?;
    let ai_advisory = classify_reply_with_ai(env, settings.as_ref(), content).await?;
    if let Some(advisory) = ai_advisory.as_ref().filter(|value| value.unsafe_detected) {
        for category in &advisory.categories {
            let ai_flag = format!("ai:{category}");
            if !result.flags.contains(&ai_flag) {
                result.flags.push(ai_flag);
            }
        }
        if result.score < 0.7 {
            result.score = 0.7;
        }
    }
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let reply_arg = D1Type::Text(reply_id);
    let status_arg = D1Type::Text(&result.status);
    let score_arg = D1Type::Real(result.score);
    let flags_json = serde_json::to_string(&result.flags).map_err(|error| error.to_string())?;
    let flags_arg = D1Type::Text(&flags_json);
    let hidden_arg = D1Type::Integer(if result.hidden { 1 } else { 0 });
    let ai_result_json = ai_advisory
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    let ai_result_arg = ai_result_json
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let ai_checked_at = if ai_advisory.is_some() {
        js_sys::Date::new_0().to_iso_string().as_string()
    } else {
        None
    };
    let ai_checked_arg = ai_checked_at
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        UPDATE replies
        SET moderation_status = ?2,
            moderation_score = ?3,
            moderation_flags = ?4,
            moderation_checked_at = CURRENT_TIMESTAMP,
            hidden = ?5,
            ai_moderation_result = ?6,
            ai_moderation_checked_at = ?7
        WHERE id = ?1
        "#,
    )
    .bind_refs([
        &reply_arg,
        &status_arg,
        &score_arg,
        &flags_arg,
        &hidden_arg,
        &ai_result_arg,
        &ai_checked_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn classify_reply_with_ai(
    env: &Env,
    settings: Option<&Map<String, Value>>,
    content: &str,
) -> std::result::Result<Option<WorkersAiModerationAdvisory>, String> {
    if !bool_field(settings, "ai_enabled") {
        return Ok(None);
    }
    let daily_budget = integer_field(settings, "ai_daily_budget");
    if daily_budget <= 0 || !ai_reply_budget_available(env, daily_budget).await? {
        return Ok(None);
    }
    let model = string_field(settings, "ai_model")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "@cf/meta/llama-guard-3-8b".to_string());
    let instruction = "Classify this social reply for advisory moderation. Return JSON only with keys unsafe (boolean), categories (array of strings), and summary (string). Categories must only use: spam, harassment, medical, adult, political, family-only, work-sensitive. Mark unsafe true only when one or more categories apply.";
    let ai = env.ai("AI").map_err(|error| error.to_string())?;
    let response: Value = ai
        .run(
            model.as_str(),
            serde_json::json!({
                "messages": [
                    { "role": "system", "content": instruction },
                    { "role": "user", "content": content }
                ],
                "max_tokens": 256,
                "temperature": 0
            }),
        )
        .await
        .map_err(|error| error.to_string())?;
    let text = workers_ai_text(&response);
    let mut advisory = parse_workers_ai_moderation(&text).unwrap_or_else(|| {
        let mut categories = Vec::new();
        let lower = text.to_ascii_lowercase();
        for category in [
            "spam",
            "harassment",
            "medical",
            "adult",
            "political",
            "family-only",
            "work-sensitive",
        ] {
            if lower.contains(category) {
                categories.push(category.to_string());
            }
        }
        WorkersAiModerationAdvisory {
            model: None,
            unsafe_detected: lower.contains("unsafe") || !categories.is_empty(),
            categories,
            summary: (!text.trim().is_empty()).then(|| truncate_text(text.trim(), 240)),
        }
    });
    advisory.model = Some(model);
    advisory.categories = normalize_ai_categories(advisory.categories);
    if advisory.summary.is_none() && !text.trim().is_empty() {
        advisory.summary = Some(truncate_text(text.trim(), 240));
    }
    Ok(Some(advisory))
}

fn classify_reply_content(
    content: &str,
    policy: &str,
) -> std::result::Result<ReplyModerationDecision, String> {
    let normalized_policy = normalize_reply_policy(policy)?;
    let lower = content.to_ascii_lowercase();
    let mut flags = Vec::new();
    let mut score: f64 = 0.0;
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("buy now")
        || lower.contains("crypto")
        || lower.contains("telegram")
        || lower.contains("whatsapp")
    {
        flags.push("spam".to_string());
        score = 0.95;
    }
    if lower.contains("kill yourself")
        || lower.contains("go die")
        || lower.contains("idiot")
        || lower.contains("stupid")
        || lower.contains("moron")
    {
        if !flags.contains(&"harassment".to_string()) {
            flags.push("harassment".to_string());
        }
        score = score.max(0.85);
    }
    for category in detect_sensitive_categories(content) {
        if !flags.contains(&category) {
            flags.push(category);
        }
        score = score.max(0.55);
    }
    let (status, hidden) = if flags.is_empty() || normalized_policy == "off" {
        ("approved".to_string(), false)
    } else {
        match normalized_policy {
            "warn" => ("approved".to_string(), false),
            "review" => ("pending".to_string(), true),
            "hide" => ("hidden".to_string(), true),
            "reject" => ("rejected".to_string(), true),
            _ => ("approved".to_string(), false),
        }
    };
    Ok(ReplyModerationDecision {
        status,
        score,
        flags,
        hidden,
    })
}

pub(crate) fn detect_sensitive_categories(content: &str) -> Vec<String> {
    let lower = content.to_ascii_lowercase();
    let mut categories = Vec::new();
    for (label, keywords) in [
        (
            "medical",
            &[
                "medical",
                "doctor",
                "clinic",
                "hospital",
                "therapy",
                "medication",
                "prescription",
                "surgery",
                "diagnosis",
                "health",
            ][..],
        ),
        (
            "adult",
            &[
                "adult", "nsfw", "sexual", "sex", "porn", "erotic", "explicit",
            ][..],
        ),
        (
            "political",
            &[
                "political",
                "politics",
                "election",
                "vote",
                "campaign",
                "senate",
                "congress",
                "democrat",
                "republican",
            ][..],
        ),
        (
            "family-only",
            &[
                "family", "kids", "child", "children", "baby", "spouse", "partner", "wedding",
            ][..],
        ),
        (
            "work-sensitive",
            &[
                "work",
                "company",
                "employer",
                "client",
                "salary",
                "interview",
                "manager",
                "confidential",
                "internal",
                "project",
            ][..],
        ),
    ] {
        if keywords.iter().any(|keyword| lower.contains(keyword)) {
            categories.push(label.to_string());
        }
    }
    categories
}

async fn ai_reply_budget_available(
    env: &Env,
    daily_budget: i64,
) -> std::result::Result<bool, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let row = db
        .prepare(
            r#"
            SELECT COUNT(*) AS count
            FROM replies
            WHERE ai_moderation_checked_at IS NOT NULL
              AND DATE(ai_moderation_checked_at) = DATE('now')
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    Ok(integer_field(row.as_ref(), "count") < daily_budget)
}

fn workers_ai_text(value: &Value) -> String {
    value
        .get("response")
        .and_then(Value::as_str)
        .or_else(|| value.get("result").and_then(Value::as_str))
        .or_else(|| {
            value
                .get("result")
                .and_then(Value::as_object)
                .and_then(|object| object.get("response"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn parse_workers_ai_moderation(text: &str) -> Option<WorkersAiModerationAdvisory> {
    let candidate = strip_json_fence(text.trim());
    let json: Value = serde_json::from_str(candidate).ok()?;
    let unsafe_detected = json
        .get("unsafe")
        .and_then(Value::as_bool)
        .or_else(|| {
            json.get("verdict")
                .and_then(Value::as_str)
                .map(|value| value.eq_ignore_ascii_case("unsafe"))
        })
        .or_else(|| {
            json.get("safe")
                .and_then(Value::as_bool)
                .map(|value| !value)
        })
        .unwrap_or(false);
    let categories = json
        .get("categories")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let summary = json
        .get("summary")
        .or_else(|| json.get("reason"))
        .and_then(Value::as_str)
        .map(|value| truncate_text(value.trim(), 240));
    Some(WorkersAiModerationAdvisory {
        model: None,
        unsafe_detected,
        categories,
        summary,
    })
}

pub(crate) fn strip_json_fence(text: &str) -> &str {
    let stripped = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .unwrap_or(text)
        .trim();
    stripped.strip_suffix("```").unwrap_or(stripped).trim()
}

pub(crate) fn normalize_ai_categories(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim().to_ascii_lowercase();
        let canonical = match trimmed.as_str() {
            "spam" | "harassment" | "medical" | "adult" | "political" | "family-only"
            | "work-sensitive" => Some(trimmed),
            "sexual" | "nsfw" | "explicit" => Some("adult".to_string()),
            "health" => Some("medical".to_string()),
            "work" => Some("work-sensitive".to_string()),
            "family" => Some("family-only".to_string()),
            _ => None,
        };
        if let Some(category) = canonical {
            if !normalized.contains(&category) {
                normalized.push(category);
            }
        }
    }
    normalized
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let shortened: String = trimmed.chars().take(max_chars).collect();
    if trimmed.chars().count() > max_chars {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn normalize_reply_policy(value: &str) -> std::result::Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Ok("off"),
        "warn" => Ok("warn"),
        "review" => Ok("review"),
        "hide" => Ok("hide"),
        "reject" => Ok("reject"),
        _ => Err("reply_policy must be one of off, warn, review, hide, reject".to_string()),
    }
}

fn normalize_reply_moderation_status(value: &str) -> std::result::Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approved" => Ok("approved".to_string()),
        "pending" => Ok("pending".to_string()),
        "hidden" => Ok("hidden".to_string()),
        "rejected" => Ok("rejected".to_string()),
        _ => Err("status must be approved, pending, hidden, or rejected".to_string()),
    }
}

async fn owner_moderation_reply(env: &Env, reply_id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let reply_arg = D1Type::Text(reply_id);
    let row = db
        .prepare(
            r#"
            SELECT id, post_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
                   content, published_at, created_at, moderation_status, moderation_score,
                   moderation_flags, moderation_checked_at, ai_moderation_result, hidden
            FROM replies
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&reply_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(row.map(shape_owner_moderation_reply))
}

fn shape_owner_moderation_reply(row: Map<String, Value>) -> Map<String, Value> {
    let mut item = row.clone();
    let flags = string_vec_json_field(Some(&row), "moderation_flags");
    item.insert(
        "moderation_flags".to_string(),
        Value::Array(flags.into_iter().map(Value::String).collect()),
    );
    if let Some(raw) = string_field(Some(&row), "ai_moderation_result") {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            item.insert("ai_moderation".to_string(), value);
        }
    }
    item
}
