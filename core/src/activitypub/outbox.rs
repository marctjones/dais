/// ActivityPub outbox - retrieve and format posts for federation
///
/// Handles queries for actor's outbox and individual posts

use crate::traits::DatabaseProvider;
use crate::error::{CoreResult, CoreError};
use serde::{Serialize, Deserialize};
use serde_json::Value;

/// A post with all its data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub actor_id: String,
    pub content: String,
    pub content_html: Option<String>,
    pub visibility: String,
    pub published_at: String,
    pub in_reply_to: Option<String>,
    pub media_attachments: Option<String>, // JSON string
    pub atproto_uri: Option<String>,
}

/// Interactions on a post (for HTML rendering)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostInteractions {
    pub replies: Vec<Reply>,
    pub likes: Vec<Interaction>,
    pub boosts: Vec<Interaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub actor_username: String,
    pub actor_display_name: String,
    pub actor_avatar_url: String,
    pub content: String,
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub actor_username: String,
    pub actor_display_name: String,
    pub actor_avatar_url: String,
    pub created_at: Option<String>,
}

/// Get all public posts for an actor's outbox
pub async fn get_outbox_posts(
    db: &dyn DatabaseProvider,
    username: &str,
) -> CoreResult<Vec<Post>> {
    // Verify actor exists
    let actor_query = "SELECT id FROM actors WHERE username = ?1";
    let actor_rows = db.execute(actor_query, &[Value::String(username.to_string())]).await?;

    if actor_rows.is_empty() {
        return Err(CoreError::NotFound(format!("Actor '{}' not found", username)));
    }

    let actor_id = actor_rows[0].get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::Internal("Missing actor id".to_string()))?
        .to_string();

    // Query for posts by this actor (public visibility only for outbox)
    let posts_query = r#"
        SELECT id, actor_id, content, content_html, visibility, published_at, in_reply_to, media_attachments, atproto_uri
        FROM posts
        WHERE actor_id = ?1 AND visibility IN ('public', 'unlisted')
        ORDER BY published_at DESC
    "#;

    let rows = db.execute(posts_query, &[Value::String(actor_id)]).await?;

    let mut posts = Vec::new();
    for row in rows {
        posts.push(Post {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_id: row.get("actor_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            content: row.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            content_html: row.get("content_html").and_then(|v| v.as_str()).map(|s| s.to_string()),
            visibility: row.get("visibility").and_then(|v| v.as_str()).unwrap_or("public").to_string(),
            published_at: row.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            in_reply_to: row.get("in_reply_to").and_then(|v| v.as_str()).map(|s| s.to_string()),
            media_attachments: row.get("media_attachments").and_then(|v| v.as_str()).map(|s| s.to_string()),
            atproto_uri: row.get("atproto_uri").and_then(|v| v.as_str()).map(|s| s.to_string()),
        });
    }

    Ok(posts)
}

/// Get a single post by ID pattern (matches /users/:username/posts/:id)
pub async fn get_post(
    db: &dyn DatabaseProvider,
    username: &str,
    post_id_param: &str,
) -> CoreResult<Post> {
    // Query for post using LIKE pattern to match the path component
    let post_path_pattern = format!("%/users/{}/posts/{}", username, post_id_param);

    let post_query = r#"
        SELECT p.id, p.actor_id, p.content, p.content_html, p.visibility,
               p.published_at, p.in_reply_to, p.media_attachments, p.atproto_uri
        FROM posts p
        JOIN actors a ON p.actor_id = a.id
        WHERE p.id LIKE ?1 AND a.username = ?2
    "#;

    let rows = db.execute(post_query, &[
        Value::String(post_path_pattern.clone()),
        Value::String(username.to_string()),
    ]).await?;

    if rows.is_empty() {
        return Err(CoreError::NotFound(format!("Post not found: {}", post_path_pattern)));
    }

    let row = &rows[0];

    Ok(Post {
        id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        actor_id: row.get("actor_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        content: row.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        content_html: row.get("content_html").and_then(|v| v.as_str()).map(|s| s.to_string()),
        visibility: row.get("visibility").and_then(|v| v.as_str()).unwrap_or("public").to_string(),
        published_at: row.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        in_reply_to: row.get("in_reply_to").and_then(|v| v.as_str()).map(|s| s.to_string()),
        media_attachments: row.get("media_attachments").and_then(|v| v.as_str()).map(|s| s.to_string()),
        atproto_uri: row.get("atproto_uri").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

/// Get interactions (replies, likes, boosts) for a post
pub async fn get_post_interactions(
    db: &dyn DatabaseProvider,
    post_id: &str,
) -> CoreResult<PostInteractions> {
    // Fetch replies (exclude hidden ones)
    let replies_query = r#"
        SELECT actor_username, actor_display_name, actor_avatar_url, content, published_at
        FROM replies
        WHERE post_id = ?1 AND (hidden IS NULL OR hidden = 0)
        ORDER BY published_at ASC
    "#;

    let reply_rows = db.execute(replies_query, &[Value::String(post_id.to_string())]).await?;
    let mut replies = Vec::new();

    for row in reply_rows {
        replies.push(Reply {
            actor_username: row.get("actor_username").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_display_name: row.get("actor_display_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_avatar_url: row.get("actor_avatar_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            content: row.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            published_at: row.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        });
    }

    // Fetch likes
    let likes_query = r#"
        SELECT actor_username, actor_display_name, actor_avatar_url
        FROM interactions
        WHERE object_id = ?1 AND type = 'like'
        ORDER BY published_at DESC
    "#;

    let like_rows = db.execute(likes_query, &[Value::String(post_id.to_string())]).await?;
    let mut likes = Vec::new();

    for row in like_rows {
        likes.push(Interaction {
            actor_username: row.get("actor_username").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_display_name: row.get("actor_display_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_avatar_url: row.get("actor_avatar_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            created_at: None,
        });
    }

    // Fetch boosts
    let boosts_query = r#"
        SELECT actor_username, actor_display_name, actor_avatar_url, published_at
        FROM interactions
        WHERE object_id = ?1 AND type = 'boost'
        ORDER BY published_at DESC
    "#;

    let boost_rows = db.execute(boosts_query, &[Value::String(post_id.to_string())]).await?;
    let mut boosts = Vec::new();

    for row in boost_rows {
        boosts.push(Interaction {
            actor_username: row.get("actor_username").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_display_name: row.get("actor_display_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor_avatar_url: row.get("actor_avatar_url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            created_at: row.get("published_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
        });
    }

    Ok(PostInteractions {
        replies,
        likes,
        boosts,
    })
}
