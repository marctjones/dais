/// ActivityPub outbox - retrieve and format posts for federation
///
/// Handles queries for actor's outbox and individual posts
use crate::activitypub::ANONYMOUS_PUBLIC_POST_SQL_PREDICATE;
use crate::error::{CoreError, CoreResult};
use crate::traits::DatabaseProvider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A post with all its data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub actor_id: String,
    pub content: String,
    pub content_html: Option<String>,
    pub object_type: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
    pub visibility: String,
    pub published_at: String,
    pub in_reply_to: Option<String>,
    pub media_attachments: Option<String>, // JSON string
    pub atproto_uri: Option<String>,
    pub encrypted_message: Option<String>,
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
pub async fn get_outbox_posts(db: &dyn DatabaseProvider, username: &str) -> CoreResult<Vec<Post>> {
    // Verify actor exists
    let actor_query = "SELECT id FROM actors WHERE username = ?1";
    let actor_rows = db
        .execute(actor_query, &[Value::String(username.to_string())])
        .await?;

    if actor_rows.is_empty() {
        return Err(CoreError::NotFound(format!(
            "Actor '{}' not found",
            username
        )));
    }

    let actor_id = actor_rows[0]
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::Internal("Missing actor id".to_string()))?
        .to_string();

    // Public outbox is an anonymous read surface. Do not list unlisted,
    // followers/direct, or encrypted fallback records here.
    let posts_query = format!(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, start_time, end_time, location, visibility, published_at, in_reply_to,
               media_attachments, atproto_uri, encrypted_message
        FROM posts
        WHERE actor_id = ?1
          AND {ANONYMOUS_PUBLIC_POST_SQL_PREDICATE}
        ORDER BY published_at DESC
    "#
    );

    let rows = db.execute(&posts_query, &[Value::String(actor_id)]).await?;

    let mut posts = Vec::new();
    for row in rows {
        posts.push(Post {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_id: row
                .get("actor_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            content: row
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            content_html: row
                .get("content_html")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            object_type: row
                .get("object_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Note")
                .to_string(),
            name: row
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            summary: row
                .get("summary")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            start_time: row
                .get("start_time")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            end_time: row
                .get("end_time")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            location: row
                .get("location")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            visibility: row
                .get("visibility")
                .and_then(|v| v.as_str())
                .unwrap_or("public")
                .to_string(),
            published_at: row
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            in_reply_to: row
                .get("in_reply_to")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            media_attachments: row
                .get("media_attachments")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            atproto_uri: row
                .get("atproto_uri")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            encrypted_message: row
                .get("encrypted_message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
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
        SELECT p.id, p.actor_id, p.content, p.content_html, COALESCE(p.object_type, 'Note') AS object_type,
               p.name, p.summary, p.start_time, p.end_time, p.location, p.visibility,
               p.published_at, p.in_reply_to, p.media_attachments, p.atproto_uri,
               p.encrypted_message
        FROM posts p
        JOIN actors a ON p.actor_id = a.id
        WHERE p.id LIKE ?1 AND a.username = ?2
    "#;

    let rows = db
        .execute(
            post_query,
            &[
                Value::String(post_path_pattern.clone()),
                Value::String(username.to_string()),
            ],
        )
        .await?;

    if rows.is_empty() {
        return Err(CoreError::NotFound(format!(
            "Post not found: {}",
            post_path_pattern
        )));
    }

    let row = &rows[0];

    Ok(Post {
        id: row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        actor_id: row
            .get("actor_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        content: row
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        content_html: row
            .get("content_html")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        object_type: row
            .get("object_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Note")
            .to_string(),
        name: row
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        summary: row
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        start_time: row
            .get("start_time")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        end_time: row
            .get("end_time")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        location: row
            .get("location")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        visibility: row
            .get("visibility")
            .and_then(|v| v.as_str())
            .unwrap_or("public")
            .to_string(),
        published_at: row
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        in_reply_to: row
            .get("in_reply_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        media_attachments: row
            .get("media_attachments")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        atproto_uri: row
            .get("atproto_uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        encrypted_message: row
            .get("encrypted_message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
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

    let reply_rows = db
        .execute(replies_query, &[Value::String(post_id.to_string())])
        .await?;
    let mut replies = Vec::new();

    for row in reply_rows {
        replies.push(Reply {
            actor_username: row
                .get("actor_username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_display_name: row
                .get("actor_display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_avatar_url: row
                .get("actor_avatar_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            content: row
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            published_at: row
                .get("published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }

    // Fetch likes
    let likes_query = r#"
        SELECT actor_username, actor_display_name, actor_avatar_url
        FROM interactions
        WHERE (post_id = ?1 OR object_url = ?1) AND type = 'like'
        ORDER BY created_at DESC
    "#;

    let like_rows = db
        .execute(likes_query, &[Value::String(post_id.to_string())])
        .await?;
    let mut likes = Vec::new();

    for row in like_rows {
        likes.push(Interaction {
            actor_username: row
                .get("actor_username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_display_name: row
                .get("actor_display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_avatar_url: row
                .get("actor_avatar_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            created_at: None,
        });
    }

    // Fetch boosts
    let boosts_query = r#"
        SELECT actor_username, actor_display_name, actor_avatar_url, created_at
        FROM interactions
        WHERE (post_id = ?1 OR object_url = ?1) AND type = 'boost'
        ORDER BY created_at DESC
    "#;

    let boost_rows = db
        .execute(boosts_query, &[Value::String(post_id.to_string())])
        .await?;
    let mut boosts = Vec::new();

    for row in boost_rows {
        boosts.push(Interaction {
            actor_username: row
                .get("actor_username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_display_name: row
                .get("actor_display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            actor_avatar_url: row
                .get("actor_avatar_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            created_at: row
                .get("created_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        });
    }

    Ok(PostInteractions {
        replies,
        likes,
        boosts,
    })
}

/// Build a Mastodon-compatible ActivityPub Note object from a core post.
pub fn build_note_object(post: &Post, interactions: Option<&PostInteractions>) -> Value {
    let followers_collection = format!("{}/followers", post.actor_id);
    let (to, cc) = note_audience(&post.visibility, &followers_collection);
    let (reply_count, like_count, boost_count) = interactions
        .map(|interactions| {
            (
                interactions.replies.len(),
                interactions.likes.len(),
                interactions.boosts.len(),
            )
        })
        .unwrap_or((0, 0, 0));

    let mut note = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": post.object_type,
        "id": post.id,
        "url": post.id,
        "attributedTo": post.actor_id,
        "content": post.content,
        "published": post.published_at,
        "to": to,
        "cc": cc,
        "replies": {
            "type": "Collection",
            "id": format!("{}/replies", post.id),
            "totalItems": reply_count
        },
        "likes": {
            "type": "Collection",
            "id": format!("{}/likes", post.id),
            "totalItems": like_count
        },
        "shares": {
            "type": "Collection",
            "id": format!("{}/shares", post.id),
            "totalItems": boost_count
        }
    });

    if let Some(ref content_html) = post.content_html {
        note["contentMap"] = json!({ "en": content_html });
    }

    if let Some(ref name) = post.name {
        note["name"] = json!(name);
    }

    if let Some(ref summary) = post.summary {
        note["summary"] = json!(summary);
    }

    if let Some(ref start_time) = post.start_time {
        note["startTime"] = json!(start_time);
    }

    if let Some(ref end_time) = post.end_time {
        note["endTime"] = json!(end_time);
    }

    if let Some(ref location) = post.location {
        note["location"] = json!({
            "type": "Place",
            "name": location
        });
    }

    if let Some(ref in_reply_to) = post.in_reply_to {
        note["inReplyTo"] = json!(in_reply_to);
    }

    if let Some(ref attachments_json) = post.media_attachments {
        if let Ok(attachments) = serde_json::from_str::<Value>(attachments_json) {
            note["attachment"] = attachments;
        }
    }

    let tags = activity_tags(&post.content);
    if !tags.is_empty() {
        note["tag"] = json!(tags);
    }

    if let Some(ref encrypted_message) = post.encrypted_message {
        if let Ok(encrypted) = serde_json::from_str::<Value>(encrypted_message) {
            note["encryptedMessage"] = encrypted;
        }
    }

    note
}

fn note_audience(visibility: &str, followers_collection: &str) -> (Vec<String>, Vec<String>) {
    match visibility {
        "public" => (
            vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
            vec![followers_collection.to_string()],
        ),
        "unlisted" => (
            vec![followers_collection.to_string()],
            vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
        ),
        "direct" => (Vec::new(), Vec::new()),
        _ => (vec![followers_collection.to_string()], Vec::new()),
    }
}

fn activity_tags(content: &str) -> Vec<Value> {
    let mut tags = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in content.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\''
            )
        });
        if let Some(tag) = hashtag_tag(trimmed) {
            if seen.insert(format!("hashtag:{trimmed}")) {
                tags.push(tag);
            }
            continue;
        }
        if let Some(tag) = mention_tag(trimmed) {
            if seen.insert(format!("mention:{trimmed}")) {
                tags.push(tag);
            }
        }
    }
    tags
}

fn hashtag_tag(token: &str) -> Option<Value> {
    let name = token.strip_prefix('#')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(json!({
        "type": "Hashtag",
        "name": format!("#{name}"),
        "href": format!("https://social.dais.social/tags/{name}")
    }))
}

fn mention_tag(token: &str) -> Option<Value> {
    let without_prefix = token.strip_prefix('@')?;
    let mut parts = without_prefix.split('@');
    let username = parts.next()?.trim();
    let host = parts.next()?.trim();
    if parts.next().is_some()
        || username.is_empty()
        || host.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
    {
        return None;
    }
    Some(json!({
        "type": "Mention",
        "name": format!("@{username}@{host}"),
        "href": format!("https://{host}/users/{username}")
    }))
}

#[cfg(test)]
mod tests {
    use super::{build_note_object, Interaction, Post, PostInteractions, Reply};

    fn post_with_visibility(visibility: &str) -> Post {
        Post {
            id: "https://social.example/users/social/posts/1".to_string(),
            actor_id: "https://social.example/users/social".to_string(),
            content: "hello".to_string(),
            content_html: None,
            object_type: "Note".to_string(),
            name: None,
            summary: None,
            start_time: None,
            end_time: None,
            location: None,
            visibility: visibility.to_string(),
            published_at: "2026-06-11T00:00:00Z".to_string(),
            in_reply_to: None,
            media_attachments: None,
            atproto_uri: None,
            encrypted_message: None,
        }
    }

    #[test]
    fn note_builder_exposes_mastodon_interaction_collections() {
        let post = post_with_visibility("public");
        let interactions = PostInteractions {
            replies: vec![Reply {
                actor_username: "alice".to_string(),
                actor_display_name: "Alice".to_string(),
                actor_avatar_url: String::new(),
                content: "reply".to_string(),
                published_at: "2026-06-11T00:01:00Z".to_string(),
            }],
            likes: vec![Interaction {
                actor_username: "bob".to_string(),
                actor_display_name: "Bob".to_string(),
                actor_avatar_url: String::new(),
                created_at: None,
            }],
            boosts: vec![Interaction {
                actor_username: "carol".to_string(),
                actor_display_name: "Carol".to_string(),
                actor_avatar_url: String::new(),
                created_at: None,
            }],
        };

        let note = build_note_object(&post, Some(&interactions));

        assert_eq!(note["type"], "Note");
        assert_eq!(note["url"], post.id);
        assert_eq!(note["replies"]["totalItems"], 1);
        assert_eq!(note["likes"]["totalItems"], 1);
        assert_eq!(note["shares"]["totalItems"], 1);
    }

    #[test]
    fn note_builder_keeps_followers_only_out_of_public_to() {
        let post = post_with_visibility("followers");
        let note = build_note_object(&post, None);

        assert_eq!(
            note["to"],
            serde_json::json!(["https://social.example/users/social/followers"])
        );
        assert_eq!(note["cc"], serde_json::json!([]));
    }

    #[test]
    fn note_builder_preserves_rich_activitypub_object_metadata() {
        let mut post = post_with_visibility("public");
        post.object_type = "Article".to_string();
        post.name = Some("A long-form title".to_string());
        post.summary = Some("Short abstract".to_string());

        let article = build_note_object(&post, None);

        assert_eq!(article["type"], "Article");
        assert_eq!(article["name"], "A long-form title");
        assert_eq!(article["summary"], "Short abstract");
        assert_eq!(article["content"], "hello");
    }

    #[test]
    fn note_builder_preserves_event_metadata() {
        let mut post = post_with_visibility("followers");
        post.object_type = "Event".to_string();
        post.name = Some("Dinner".to_string());
        post.summary = Some("Small private dinner".to_string());
        post.start_time = Some("2026-06-12T18:00:00Z".to_string());
        post.end_time = Some("2026-06-12T20:00:00Z".to_string());
        post.location = Some("Kitchen table".to_string());

        let event = build_note_object(&post, None);

        assert_eq!(event["type"], "Event");
        assert_eq!(event["name"], "Dinner");
        assert_eq!(event["startTime"], "2026-06-12T18:00:00Z");
        assert_eq!(event["endTime"], "2026-06-12T20:00:00Z");
        assert_eq!(event["location"]["type"], "Place");
        assert_eq!(event["location"]["name"], "Kitchen table");
        assert_eq!(
            event["to"],
            serde_json::json!(["https://social.example/users/social/followers"])
        );
    }

    #[test]
    fn note_builder_exposes_mastodon_mentions_and_hashtags() {
        let mut post = post_with_visibility("public");
        post.content = "hello @alice@example.social #Dais".to_string();
        let note = build_note_object(&post, None);

        assert_eq!(note["tag"][0]["type"], "Mention");
        assert_eq!(note["tag"][0]["name"], "@alice@example.social");
        assert_eq!(note["tag"][1]["type"], "Hashtag");
        assert_eq!(note["tag"][1]["name"], "#Dais");
    }
}
