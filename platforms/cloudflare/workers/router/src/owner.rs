use crate::config::{local_actor_url, owner_instance_url};
use crate::request::{optional_body_string, optional_url_field, string_like_any};
use crate::{
    column_exists, integer_field, js_truthy, normalize_protocol, normalize_visibility,
    string_field, title_protocol, title_visibility,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use worker::{D1Type, Env, Result};

#[derive(Serialize)]
pub(crate) struct OwnerProfile {
    pub(crate) id: String,
    pub(crate) username: String,
    pub(crate) actor_type: String,
    pub(crate) display_name: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) header_url: Option<String>,
    pub(crate) public_handle: String,
    pub(crate) actor_url: String,
}

#[derive(Serialize)]
pub(crate) struct OwnerStats {
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
pub(crate) struct OwnerDiagnostic {
    key: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Deserialize)]
struct DeliveryCount {
    status: String,
    count: i64,
}

pub(crate) async fn owner_profile(env: &Env) -> Result<OwnerProfile> {
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

pub(crate) async fn owner_update_profile(
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

pub(crate) async fn owner_stats(env: &Env) -> Result<OwnerStats> {
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

pub(crate) async fn owner_diagnostics(env: &Env) -> Result<Vec<OwnerDiagnostic>> {
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

pub(crate) async fn owner_settings(env: &Env) -> Result<Map<String, Value>> {
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

pub(crate) async fn owner_snapshot_settings(env: &Env) -> Result<Map<String, Value>> {
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

pub(crate) async fn owner_update_settings(
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
