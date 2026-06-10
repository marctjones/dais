use crate::error::CoreResult;
use crate::traits::DatabaseProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePost {
    pub object_id: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub content: String,
    pub content_html: Option<String>,
    pub visibility: String,
    pub in_reply_to: Option<String>,
    pub published_at: String,
    pub updated_at: Option<String>,
    pub protocol: String,
    pub encrypted_message: Option<String>,
}

pub async fn get_home_timeline(
    db: &dyn DatabaseProvider,
    limit: u32,
    before: Option<&str>,
) -> CoreResult<Vec<TimelinePost>> {
    let limit = limit.clamp(1, 200);
    let (query, params) = if let Some(before) = before {
        (
            format!(
                r#"
                SELECT object_id, actor_id, actor_username, actor_display_name,
                       actor_avatar_url, content, content_html, visibility,
                       in_reply_to, published_at, updated_at, protocol, encrypted_message
                FROM timeline_posts
                WHERE deleted_at IS NULL AND published_at < ?1
                ORDER BY published_at DESC
                LIMIT {limit}
                "#
            ),
            vec![Value::String(before.to_string())],
        )
    } else {
        (
            format!(
                r#"
                SELECT object_id, actor_id, actor_username, actor_display_name,
                       actor_avatar_url, content, content_html, visibility,
                       in_reply_to, published_at, updated_at, protocol, encrypted_message
                FROM timeline_posts
                WHERE deleted_at IS NULL
                ORDER BY published_at DESC
                LIMIT {limit}
                "#
            ),
            Vec::new(),
        )
    };

    let rows = db.execute(&query, &params).await?;
    Ok(rows
        .into_iter()
        .map(|row| TimelinePost {
            object_id: row.get_string("object_id").unwrap_or_default(),
            actor_id: row.get_string("actor_id").unwrap_or_default(),
            actor_username: row.get_string("actor_username"),
            actor_display_name: row.get_string("actor_display_name"),
            actor_avatar_url: row.get_string("actor_avatar_url"),
            content: row.get_string("content").unwrap_or_default(),
            content_html: row.get_string("content_html"),
            visibility: row
                .get_string("visibility")
                .unwrap_or_else(|| "unknown".to_string()),
            in_reply_to: row.get_string("in_reply_to"),
            published_at: row.get_string("published_at").unwrap_or_default(),
            updated_at: row.get_string("updated_at"),
            protocol: row
                .get_string("protocol")
                .unwrap_or_else(|| "activitypub".to_string()),
            encrypted_message: row.get_string("encrypted_message"),
        })
        .collect())
}
