/// ActivityPub inbox processing
///
/// Handles incoming activities from remote servers

use crate::traits::{DatabaseProvider, HttpProvider};
use crate::error::{CoreResult, CoreError};
use crate::activitypub::types::Activity;
use serde_json::Value;
use std::collections::HashMap;

/// Content moderation result
#[derive(Debug, Clone)]
pub struct ModerationResult {
    pub status: String,      // "approved", "flagged", "rejected"
    pub score: f64,          // 0.0-1.0 confidence score
    pub flags: String,       // Comma-separated flags
    pub hidden: bool,        // Should be hidden from display
}

/// Trait for content moderation (platform-specific)
#[async_trait::async_trait(?Send)]
pub trait ContentModerator {
    async fn moderate(&self, content: &str) -> CoreResult<ModerationResult>;
}

/// Check if an actor is blocked
pub async fn is_blocked(
    db: &dyn DatabaseProvider,
    actor_url: &str,
) -> CoreResult<bool> {
    // Schema: the moderation table is `blocks` keyed by `actor_id` (the blocked
    // actor's URL), not `blocked_actors`/`actor_url`.
    let query = "SELECT COUNT(*) as count FROM blocks WHERE actor_id = ?1";
    let rows = db.execute(query, &[Value::String(actor_url.to_string())]).await?;

    if !rows.is_empty() {
        if let Some(count) = rows[0].get("count").and_then(|v| v.as_u64()) {
            return Ok(count > 0);
        }
    }

    Ok(false)
}

/// Extract actor info from actor URL (fetch remote actor profile)
pub async fn extract_actor_info(
    http: &dyn HttpProvider,
    actor_url: &str,
) -> CoreResult<(String, String, String)> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct ActorProfile {
        #[serde(rename = "preferredUsername")]
        preferred_username: String,
        name: Option<String>,
        icon: Option<IconImage>,
    }

    #[derive(Deserialize)]
    struct IconImage {
        url: String,
    }

    // Build request
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/activity+json".to_string());

    let request = crate::traits::Request {
        url: actor_url.to_string(),
        method: crate::traits::Method::Get,
        headers,
        body: None,
        timeout: Some(30),
        follow_redirects: true,
    };

    // Fetch actor profile
    let response = http.fetch(request).await?;

    if response.status < 200 || response.status >= 300 {
        // Return defaults if fetch fails
        return Ok((
            "unknown".to_string(),
            "Unknown User".to_string(),
            "".to_string(),
        ));
    }

    // Parse response
    let json_str = String::from_utf8(response.body)
        .map_err(|e| CoreError::Serialization(format!("Invalid UTF-8: {}", e)))?;

    let actor: ActorProfile = serde_json::from_str(&json_str)
        .map_err(|_| CoreError::Internal("Failed to parse actor profile".to_string()))?;

    Ok((
        actor.preferred_username,
        actor.name.unwrap_or_else(|| "Unknown".to_string()),
        actor.icon.map(|i| i.url).unwrap_or_default(),
    ))
}

/// Create a notification in the database
pub async fn create_notification(
    db: &dyn DatabaseProvider,
    notification_type: &str,
    actor_id: &str,
    actor_username: &str,
    actor_display_name: &str,
    actor_avatar_url: &str,
    post_id: Option<&str>,
    reply_id: Option<&str>,
    content: Option<&str>,
) -> CoreResult<()> {
    let id = crate::utils::generate_uuid();
    let created_at = crate::utils::now_rfc3339();

    let query = r#"
        INSERT INTO notifications (
            id, type, actor_id, actor_username, actor_display_name,
            actor_avatar_url, post_id, activity_id, content, created_at, read
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
    "#;

    db.execute(query, &[
        Value::String(id),
        Value::String(notification_type.to_string()),
        Value::String(actor_id.to_string()),
        Value::String(actor_username.to_string()),
        Value::String(actor_display_name.to_string()),
        Value::String(actor_avatar_url.to_string()),
        post_id.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        reply_id.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        content.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        Value::String(created_at),
    ]).await?;

    Ok(())
}

/// Handle Follow activity
pub async fn handle_follow(
    db: &dyn DatabaseProvider,
    activity: &Activity,
    our_actor_url: &str,
) -> CoreResult<()> {
    // Extract follower's inbox from their actor object
    let follower_inbox = format!("{}/inbox", activity.actor);

    // Insert into followers table with 'pending' status
    let query = r#"
        INSERT OR IGNORE INTO followers (
            id, actor_id, follower_actor_id, follower_inbox, status
        ) VALUES (?1, ?2, ?3, ?4, 'pending')
    "#;

    db.execute(query, &[
        Value::String(activity.id.clone()),
        Value::String(our_actor_url.to_string()),
        Value::String(activity.actor.clone()),
        Value::String(follower_inbox),
    ]).await?;

    Ok(())
}

/// Handle Undo activity
pub async fn handle_undo(
    db: &dyn DatabaseProvider,
    activity: &Activity,
) -> CoreResult<()> {
    // The object should be the activity being undone
    if let Some(object_type) = activity.object.as_ref().and_then(|o| o.get("type")).and_then(|v| v.as_str()) {
        match object_type {
            "Follow" => {
                // Remove the follower
                let query = "DELETE FROM followers WHERE follower_actor_id = ?1";
                db.execute(query, &[Value::String(activity.actor.clone())]).await?;
            }
            "Like" => {
                // Remove the like
                if let Some(object_id) = activity.object.as_ref().and_then(|o| o.get("id")).and_then(|v| v.as_str()) {
                    let query = "DELETE FROM interactions WHERE id = ?1";
                    db.execute(query, &[Value::String(object_id.to_string())]).await?;
                }
            }
            "Announce" => {
                // Remove the boost
                if let Some(object_id) = activity.object.as_ref().and_then(|o| o.get("id")).and_then(|v| v.as_str()) {
                    let query = "DELETE FROM interactions WHERE id = ?1";
                    db.execute(query, &[Value::String(object_id.to_string())]).await?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// Handle Create activity (posts, replies, DMs)
pub async fn handle_create(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
    our_actor_url: &str,
    moderator: Option<&dyn ContentModerator>,
) -> CoreResult<()> {
    // Check if the object is a Note (post/reply)
    if let Some(object_type) = activity.object.as_ref().and_then(|o| o.get("type")).and_then(|v| v.as_str()) {
        if object_type != "Note" {
            return Ok(()); // Not a Note, nothing to do
        }
    } else {
        return Ok(());
    }

    let object = activity.object.as_ref().ok_or_else(|| CoreError::InvalidActivity("Missing object".to_string()))?;

    // Check if this is a DM (to contains our actor, no Public)
    let is_dm = object.get("to")
        .and_then(|to| to.as_array())
        .map(|to_array| {
            let has_our_actor = to_array.iter().any(|recipient| {
                recipient.as_str() == Some(our_actor_url)
            });
            let has_public = to_array.iter().any(|recipient| {
                recipient.as_str() == Some("https://www.w3.org/ns/activitystreams#Public")
            });
            has_our_actor && !has_public
        })
        .unwrap_or(false);

    if is_dm {
        return handle_direct_message(db, http, activity, our_actor_url).await;
    }

    // Check if this is a reply to one of our posts
    if let Some(in_reply_to) = object.get("inReplyTo").and_then(|v| v.as_str()) {
        return handle_reply(db, http, activity, in_reply_to, moderator).await;
    }

    if is_accepted_following(db, &activity.actor).await? {
        return ingest_timeline_post(db, http, activity).await;
    }

    Ok(())
}

/// Handle Update activity for timeline Notes from accepted follows.
pub async fn handle_update(
    db: &dyn DatabaseProvider,
    activity: &Activity,
) -> CoreResult<()> {
    let Some(object) = activity.object.as_ref() else {
        return Ok(());
    };
    if object.get("type").and_then(|v| v.as_str()) != Some("Note") {
        return Ok(());
    }
    if !is_accepted_following(db, &activity.actor).await? {
        return Ok(());
    }

    let object_id = note_object_id(activity)?;
    let content = object.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let updated_at = object
        .get("updated")
        .and_then(|v| v.as_str())
        .or_else(|| activity.published.as_deref())
        .map(|value| value.to_string())
        .unwrap_or_else(crate::utils::now_rfc3339);
    let raw_object = serde_json::to_string(object)?;

    let query = r#"
        UPDATE timeline_posts
        SET content = ?1,
            content_html = ?2,
            updated_at = ?3,
            raw_object = ?4,
            deleted_at = NULL
        WHERE object_id = ?5
    "#;

    db.execute(query, &[
        Value::String(content.to_string()),
        Value::String(crate::utils::sanitize_html(content)),
        Value::String(updated_at),
        Value::String(raw_object),
        Value::String(object_id),
    ]).await?;

    Ok(())
}

/// Handle Delete activity for timeline Notes from accepted follows.
pub async fn handle_delete(
    db: &dyn DatabaseProvider,
    activity: &Activity,
) -> CoreResult<()> {
    if !is_accepted_following(db, &activity.actor).await? {
        return Ok(());
    }

    let object_id = match activity.object.as_ref() {
        Some(Value::String(id)) => id.to_string(),
        Some(object) => object
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(&activity.id)
            .to_string(),
        None => return Ok(()),
    };

    let deleted_at = activity
        .published
        .clone()
        .unwrap_or_else(crate::utils::now_rfc3339);

    let query = "UPDATE timeline_posts SET deleted_at = ?1 WHERE object_id = ?2";
    db.execute(query, &[
        Value::String(deleted_at),
        Value::String(object_id),
    ]).await?;

    Ok(())
}

async fn is_accepted_following(
    db: &dyn DatabaseProvider,
    actor_id: &str,
) -> CoreResult<bool> {
    let query = "SELECT COUNT(*) AS count FROM following WHERE target_actor_id = ?1 AND status = 'accepted'";
    let rows = db.execute(query, &[Value::String(actor_id.to_string())]).await?;
    Ok(rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) > 0)
}

async fn ingest_timeline_post(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
) -> CoreResult<()> {
    let object = activity.object.as_ref().ok_or_else(|| CoreError::InvalidActivity("Missing object".to_string()))?;
    let object_id = note_object_id(activity)?;
    let content = object.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let published_at = object
        .get("published")
        .and_then(|v| v.as_str())
        .or_else(|| activity.published.as_deref())
        .map(|value| value.to_string())
        .unwrap_or_else(crate::utils::now_rfc3339);
    let visibility = infer_note_visibility(object);
    let in_reply_to = object.get("inReplyTo").and_then(|v| v.as_str());
    let raw_object = serde_json::to_string(object)?;
    let raw_activity = serde_json::to_string(activity)?;

    let (actor_username, actor_display_name, actor_avatar_url) =
        extract_actor_info(http, &activity.actor).await.unwrap_or_else(|_| {
            ("unknown".to_string(), "Unknown".to_string(), "".to_string())
        });

    let query = r#"
        INSERT INTO timeline_posts (
            id, object_id, actor_id, actor_username, actor_display_name,
            actor_avatar_url, content, content_html, visibility, in_reply_to,
            published_at, raw_object, raw_activity, protocol
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'activitypub')
        ON CONFLICT(object_id) DO UPDATE SET
            actor_id = excluded.actor_id,
            actor_username = excluded.actor_username,
            actor_display_name = excluded.actor_display_name,
            actor_avatar_url = excluded.actor_avatar_url,
            content = excluded.content,
            content_html = excluded.content_html,
            visibility = excluded.visibility,
            in_reply_to = excluded.in_reply_to,
            published_at = excluded.published_at,
            raw_object = excluded.raw_object,
            raw_activity = excluded.raw_activity,
            deleted_at = NULL
    "#;

    db.execute(query, &[
        Value::String(crate::utils::generate_uuid()),
        Value::String(object_id),
        Value::String(activity.actor.clone()),
        Value::String(actor_username),
        Value::String(actor_display_name),
        Value::String(actor_avatar_url),
        Value::String(content.to_string()),
        Value::String(crate::utils::sanitize_html(content)),
        Value::String(visibility),
        in_reply_to.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        Value::String(published_at),
        Value::String(raw_object),
        Value::String(raw_activity),
    ]).await?;

    Ok(())
}

fn note_object_id(activity: &Activity) -> CoreResult<String> {
    activity
        .object
        .as_ref()
        .and_then(|object| object.get("id"))
        .and_then(|v| v.as_str())
        .map(|id| id.to_string())
        .ok_or_else(|| CoreError::InvalidActivity("Missing Note id".to_string()))
}

fn infer_note_visibility(object: &Value) -> String {
    let has_public = |field: &str| {
        object
            .get(field)
            .and_then(|value| value.as_array())
            .map(|values| {
                values.iter().any(|value| {
                    value.as_str() == Some("https://www.w3.org/ns/activitystreams#Public")
                })
            })
            .unwrap_or(false)
    };

    if has_public("to") {
        "public".to_string()
    } else if has_public("cc") {
        "unlisted".to_string()
    } else {
        "followers".to_string()
    }
}

/// Handle direct message
async fn handle_direct_message(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
    our_actor_url: &str,
) -> CoreResult<()> {
    let object = activity.object.as_ref().ok_or_else(|| CoreError::InvalidActivity("Missing object".to_string()))?;

    let dm_id = object.get("id").and_then(|v| v.as_str()).unwrap_or(&activity.id);
    let content = object.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let published_at = object.get("published").and_then(|v| v.as_str()).unwrap_or("");

    // Fetch actor info
    let (actor_username, actor_display_name, actor_avatar_url) =
        extract_actor_info(http, &activity.actor).await.unwrap_or_else(|_| {
            ("unknown".to_string(), "Unknown".to_string(), "".to_string())
        });

    // Store the DM
    let query = r#"
        INSERT OR IGNORE INTO direct_messages (
            id, from_actor_id, to_actor_id, from_username, from_display_name,
            from_avatar_url, content, published_at, read
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
    "#;

    db.execute(query, &[
        Value::String(dm_id.to_string()),
        Value::String(activity.actor.clone()),
        Value::String(our_actor_url.to_string()),
        Value::String(actor_username.clone()),
        Value::String(actor_display_name.clone()),
        Value::String(actor_avatar_url.clone()),
        Value::String(content.to_string()),
        Value::String(published_at.to_string()),
    ]).await?;

    // Create notification
    create_notification(
        db,
        "dm",
        &activity.actor,
        &actor_username,
        &actor_display_name,
        &actor_avatar_url,
        None,
        Some(dm_id),
        Some(content),
    ).await?;

    Ok(())
}

/// Handle reply to our post
async fn handle_reply(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
    in_reply_to: &str,
    moderator: Option<&dyn ContentModerator>,
) -> CoreResult<()> {
    let object = activity.object.as_ref().ok_or_else(|| CoreError::InvalidActivity("Missing object".to_string()))?;

    let reply_id = object.get("id").and_then(|v| v.as_str()).unwrap_or(&activity.id);
    let content = object.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let published_at = object.get("published").and_then(|v| v.as_str()).unwrap_or("");

    // Fetch actor info
    let (actor_username, actor_display_name, actor_avatar_url) =
        extract_actor_info(http, &activity.actor).await.unwrap_or_else(|_| {
            ("unknown".to_string(), "Unknown".to_string(), "".to_string())
        });

    // Check if in_reply_to is one of our posts
    let our_post_query = "SELECT id FROM posts WHERE id = ?1";
    let our_post_result = db.execute(our_post_query, &[Value::String(in_reply_to.to_string())]).await?;

    if our_post_result.is_empty() {
        return Ok(()); // Not a reply to our post
    }

    // Run moderation if available
    let (moderation_status, moderation_score, moderation_flags, hidden) = if let Some(mod_service) = moderator {
        let result = mod_service.moderate(content).await.unwrap_or(ModerationResult {
            status: "approved".to_string(),
            score: 0.0,
            flags: "".to_string(),
            hidden: false,
        });
        (result.status, result.score, result.flags, result.hidden)
    } else {
        ("approved".to_string(), 0.0, "".to_string(), false)
    };

    let checked_at = crate::utils::now_rfc3339();

    // Store the reply
    let insert_query = r#"
        INSERT OR IGNORE INTO replies (
            id, post_id, actor_id, actor_username, actor_display_name,
            actor_avatar_url, content, published_at,
            moderation_status, moderation_score, moderation_flags,
            moderation_checked_at, hidden
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
    "#;

    db.execute(insert_query, &[
        Value::String(reply_id.to_string()),
        Value::String(in_reply_to.to_string()),
        Value::String(activity.actor.clone()),
        Value::String(actor_username.clone()),
        Value::String(actor_display_name.clone()),
        Value::String(actor_avatar_url.clone()),
        Value::String(content.to_string()),
        Value::String(published_at.to_string()),
        Value::String(moderation_status),
        Value::Number(serde_json::Number::from_f64(moderation_score).unwrap()),
        Value::String(moderation_flags),
        Value::String(checked_at),
        Value::Bool(hidden),
    ]).await?;

    // Create notification only if not hidden
    if !hidden {
        create_notification(
            db,
            "reply",
            &activity.actor,
            &actor_username,
            &actor_display_name,
            &actor_avatar_url,
            Some(in_reply_to),
            Some(reply_id),
            Some(content),
        ).await?;
    }

    Ok(())
}

/// Handle Like activity
pub async fn handle_like(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
) -> CoreResult<()> {
    // Extract the object being liked
    let object_id = activity.object.as_ref()
        .and_then(|o| o.as_str())
        .ok_or_else(|| CoreError::InvalidActivity("Missing object ID".to_string()))?;

    // Fetch actor info
    let (actor_username, actor_display_name, actor_avatar_url) =
        extract_actor_info(http, &activity.actor).await.unwrap_or_else(|_| {
            ("unknown".to_string(), "Unknown".to_string(), "".to_string())
        });

    let published_at = activity.published.as_deref().unwrap_or("");

    // Store the like
    let query = r#"
        INSERT OR IGNORE INTO interactions (
            id, type, actor_id, actor_username, actor_display_name,
            actor_avatar_url, object_url, created_at
        ) VALUES (?1, 'like', ?2, ?3, ?4, ?5, ?6, ?7)
    "#;

    db.execute(query, &[
        Value::String(activity.id.clone()),
        Value::String(activity.actor.clone()),
        Value::String(actor_username.clone()),
        Value::String(actor_display_name.clone()),
        Value::String(actor_avatar_url.clone()),
        Value::String(object_id.to_string()),
        Value::String(published_at.to_string()),
    ]).await?;

    // Create notification
    create_notification(
        db,
        "like",
        &activity.actor,
        &actor_username,
        &actor_display_name,
        &actor_avatar_url,
        Some(object_id),
        None,
        None,
    ).await?;

    Ok(())
}

/// Handle Announce activity (boost/reblog)
pub async fn handle_announce(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: &Activity,
) -> CoreResult<()> {
    // Extract the object being announced
    let object_id = activity.object.as_ref()
        .and_then(|o| o.as_str())
        .ok_or_else(|| CoreError::InvalidActivity("Missing object ID".to_string()))?;

    // Fetch actor info
    let (actor_username, actor_display_name, actor_avatar_url) =
        extract_actor_info(http, &activity.actor).await.unwrap_or_else(|_| {
            ("unknown".to_string(), "Unknown".to_string(), "".to_string())
        });

    let published_at = activity.published.as_deref().unwrap_or("");

    // Store the boost
    let query = r#"
        INSERT OR IGNORE INTO interactions (
            id, type, actor_id, actor_username, actor_display_name,
            actor_avatar_url, object_url, created_at
        ) VALUES (?1, 'boost', ?2, ?3, ?4, ?5, ?6, ?7)
    "#;

    db.execute(query, &[
        Value::String(activity.id.clone()),
        Value::String(activity.actor.clone()),
        Value::String(actor_username.clone()),
        Value::String(actor_display_name.clone()),
        Value::String(actor_avatar_url.clone()),
        Value::String(object_id.to_string()),
        Value::String(published_at.to_string()),
    ]).await?;

    // Create notification
    create_notification(
        db,
        "boost",
        &activity.actor,
        &actor_username,
        &actor_display_name,
        &actor_avatar_url,
        Some(object_id),
        None,
        None,
    ).await?;

    Ok(())
}

/// Handle Accept activity (follow request approved)
pub async fn handle_accept(
    db: &dyn DatabaseProvider,
    activity: &Activity,
) -> CoreResult<()> {
    // The object should be our Follow activity
    if let Some(object_type) = activity.object.as_ref().and_then(|o| o.get("type")).and_then(|v| v.as_str()) {
        if object_type == "Follow" {
            // Update the following status to approved
            let query = "UPDATE following SET status = 'accepted', accepted_at = CURRENT_TIMESTAMP WHERE target_actor_id = ?1 AND status = 'pending'";
            db.execute(query, &[Value::String(activity.actor.clone())]).await?;
        }
    }

    Ok(())
}

/// Handle Reject activity (follow request rejected)
pub async fn handle_reject(
    db: &dyn DatabaseProvider,
    activity: &Activity,
) -> CoreResult<()> {
    // The object should be our Follow activity
    if let Some(object_type) = activity.object.as_ref().and_then(|o| o.get("type")).and_then(|v| v.as_str()) {
        if object_type == "Follow" {
            // Remove the follow request
            let query = "DELETE FROM following WHERE target_actor_id = ?1 AND status = 'pending'";
            db.execute(query, &[Value::String(activity.actor.clone())]).await?;
        }
    }

    Ok(())
}

/// Main inbox handler - routes activity to appropriate handler
pub async fn process_inbox_activity(
    db: &dyn DatabaseProvider,
    http: &dyn HttpProvider,
    activity: Activity,
    our_actor_url: &str,
    moderator: Option<&dyn ContentModerator>,
) -> CoreResult<()> {
    // Check if actor is blocked
    if is_blocked(db, &activity.actor).await? {
        return Err(CoreError::Unauthorized(format!("Actor is blocked: {}", activity.actor)));
    }

    // Route to appropriate handler based on activity type
    match activity.activity_type.as_str() {
        "Follow" => handle_follow(db, &activity, our_actor_url).await?,
        "Undo" => handle_undo(db, &activity).await?,
        "Create" => handle_create(db, http, &activity, our_actor_url, moderator).await?,
        "Update" => handle_update(db, &activity).await?,
        "Delete" => handle_delete(db, &activity).await?,
        "Like" => handle_like(db, http, &activity).await?,
        "Announce" => handle_announce(db, http, &activity).await?,
        "Accept" => handle_accept(db, &activity).await?,
        "Reject" => handle_reject(db, &activity).await?,
        _ => {
            // Unsupported activity type - just log and ignore
        }
    }

    Ok(())
}
