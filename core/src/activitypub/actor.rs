/// Platform-agnostic actor logic for ActivityPub
///
/// This module handles:
/// - Fetching actor data from database
/// - Building Person objects
/// - Managing followers/following collections

use crate::traits::DatabaseProvider;
use crate::error::{CoreResult, CoreError};
use crate::activitypub::types::Person;
use serde_json::Value;

/// Actor data from database
#[derive(Debug, Clone)]
pub struct ActorData {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub public_key: String,
    pub icon: Option<String>,
    pub image: Option<String>,
}

/// Collection counts for an actor
#[derive(Debug, Clone)]
pub struct ActorCounts {
    pub post_count: u64,
    pub follower_count: u64,
    pub following_count: u64,
}

/// Get actor by username
pub async fn get_actor(
    db: &dyn DatabaseProvider,
    username: &str,
    domain: &str,
) -> CoreResult<Person> {
    // Query for actor
    let query = "SELECT id, username, display_name, summary, public_key, icon, image FROM actors WHERE username = ?1";
    let rows = db.execute(query, &[Value::String(username.to_string())]).await?;

    if rows.is_empty() {
        return Err(CoreError::NotFound(format!("Actor '{}' not found", username)));
    }

    let row = &rows[0];

    // Extract fields from row
    let actor_username = row.get("username")
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or_else(|| CoreError::Internal("Missing username field".to_string()))?;

    let public_key_pem = row.get("public_key")
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .ok_or_else(|| CoreError::Internal("Missing public_key field".to_string()))?;

    // Build Person object
    let mut person = Person::new(
        format!("unused"), // ID will be set by constructor
        actor_username.clone(),
        domain.to_string(),
        public_key_pem,
    );

    // Add optional fields
    if let Some(Value::String(name)) = row.get("display_name") {
        if !name.is_empty() {
            person = person.with_name(name.clone());
        }
    }

    if let Some(Value::String(summary)) = row.get("summary") {
        if !summary.is_empty() {
            person = person.with_summary(summary.clone());
        }
    }

    if let Some(Value::String(icon_url)) = row.get("icon") {
        if !icon_url.is_empty() {
            person = person.with_icon(icon_url.clone());
        }
    }

    if let Some(Value::String(image_url)) = row.get("image") {
        if !image_url.is_empty() {
            person = person.with_header(image_url.clone());
        }
    }

    Ok(person)
}

/// Get actor counts (posts, followers, following)
pub async fn get_actor_counts(
    db: &dyn DatabaseProvider,
    actor_id: &str,
) -> CoreResult<ActorCounts> {
    // Query for post count
    let post_count_query = "SELECT COUNT(*) as count FROM posts WHERE actor_id = ?1";
    let post_rows = db.execute(post_count_query, &[Value::String(actor_id.to_string())]).await?;

    let post_count = if !post_rows.is_empty() {
        post_rows[0].get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    } else {
        0
    };

    // Query for follower count
    let follower_count_query = "SELECT COUNT(*) as count FROM followers WHERE actor_id = ?1 AND status = 'approved'";
    let follower_rows = db.execute(follower_count_query, &[Value::String(actor_id.to_string())]).await?;

    let follower_count = if !follower_rows.is_empty() {
        follower_rows[0].get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    } else {
        0
    };

    // Query for following count
    let following_count_query = "SELECT COUNT(*) as count FROM following WHERE actor_id = ?1 AND status = 'approved'";
    let following_rows = db.execute(following_count_query, &[Value::String(actor_id.to_string())]).await?;

    let following_count = if !following_rows.is_empty() {
        following_rows[0].get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    } else {
        0
    };

    Ok(ActorCounts {
        post_count,
        follower_count,
        following_count,
    })
}

/// Get followers collection for an actor
pub async fn get_followers(
    db: &dyn DatabaseProvider,
    username: &str,
    domain: &str,
    page: Option<u32>,
) -> CoreResult<serde_json::Value> {
    let actor_url = format!("https://{}/users/{}", domain, username);

    if let Some(page_num) = page {
        // Return paginated collection page
        let items_per_page = 50;
        let offset = (page_num.saturating_sub(1)) * items_per_page;

        let query = format!(
            "SELECT follower_actor_id FROM followers WHERE actor_id = ?1 AND status = 'approved' ORDER BY created_at DESC LIMIT {} OFFSET {}",
            items_per_page, offset
        );

        let rows = db.execute(&query, &[Value::String(actor_url.clone())]).await?;

        let items: Vec<String> = rows.iter()
            .filter_map(|row| {
                row.get("follower_actor_id").and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "OrderedCollectionPage",
            "id": format!("https://{}/users/{}/followers?page={}", domain, username, page_num),
            "partOf": format!("https://{}/users/{}/followers", domain, username),
            "orderedItems": items
        }))
    } else {
        // Return collection summary
        let count_query = "SELECT COUNT(*) as count FROM followers WHERE actor_id = ?1 AND status = 'approved'";
        let rows = db.execute(count_query, &[Value::String(actor_url.clone())]).await?;

        let total_items = if !rows.is_empty() {
            rows[0].get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        } else {
            0
        };

        Ok(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "OrderedCollection",
            "id": format!("https://{}/users/{}/followers", domain, username),
            "totalItems": total_items,
            "first": format!("https://{}/users/{}/followers?page=1", domain, username)
        }))
    }
}

/// Get following collection for an actor
pub async fn get_following(
    db: &dyn DatabaseProvider,
    username: &str,
    domain: &str,
    page: Option<u32>,
) -> CoreResult<serde_json::Value> {
    let actor_url = format!("https://{}/users/{}", domain, username);

    if let Some(page_num) = page {
        // Return paginated collection page
        let items_per_page = 50;
        let offset = (page_num.saturating_sub(1)) * items_per_page;

        let query = format!(
            "SELECT target_actor_id FROM following WHERE actor_id = ?1 AND status = 'approved' ORDER BY created_at DESC LIMIT {} OFFSET {}",
            items_per_page, offset
        );

        let rows = db.execute(&query, &[Value::String(actor_url.clone())]).await?;

        let items: Vec<String> = rows.iter()
            .filter_map(|row| {
                row.get("target_actor_id").and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "OrderedCollectionPage",
            "id": format!("https://{}/users/{}/following?page={}", domain, username, page_num),
            "partOf": format!("https://{}/users/{}/following", domain, username),
            "orderedItems": items
        }))
    } else {
        // Return collection summary
        let count_query = "SELECT COUNT(*) as count FROM following WHERE actor_id = ?1 AND status = 'approved'";
        let rows = db.execute(count_query, &[Value::String(actor_url.clone())]).await?;

        let total_items = if !rows.is_empty() {
            rows[0].get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        } else {
            0
        };

        Ok(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "OrderedCollection",
            "id": format!("https://{}/users/{}/following", domain, username),
            "totalItems": total_items,
            "first": format!("https://{}/users/{}/following?page=1", domain, username)
        }))
    }
}
