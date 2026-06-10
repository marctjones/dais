use crate::error::CoreResult;
use crate::traits::DatabaseProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Friend {
    pub local_actor_id: String,
    pub friend_actor_id: String,
    pub friend_inbox: Option<String>,
    pub friend_shared_inbox: Option<String>,
    pub follower_since: Option<String>,
    pub following_since: Option<String>,
    pub accepted_at: Option<String>,
}

pub async fn get_friends(
    db: &dyn DatabaseProvider,
    local_actor_id: &str,
    limit: u32,
) -> CoreResult<Vec<Friend>> {
    let limit = limit.clamp(1, 200);
    let query = format!(
        r#"
        SELECT local_actor_id, friend_actor_id, friend_inbox, friend_shared_inbox,
               follower_since, following_since, accepted_at
        FROM friends
        WHERE local_actor_id = ?1
        ORDER BY COALESCE(accepted_at, following_since, follower_since) DESC
        LIMIT {limit}
        "#
    );

    let rows = db
        .execute(&query, &[Value::String(local_actor_id.to_string())])
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| Friend {
            local_actor_id: row.get_string("local_actor_id").unwrap_or_default(),
            friend_actor_id: row.get_string("friend_actor_id").unwrap_or_default(),
            friend_inbox: row.get_string("friend_inbox"),
            friend_shared_inbox: row.get_string("friend_shared_inbox"),
            follower_since: row.get_string("follower_since"),
            following_since: row.get_string("following_since"),
            accepted_at: row.get_string("accepted_at"),
        })
        .collect())
}
