use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Post {
    pub id: String,
    pub content: String,
    pub visibility: Option<String>,
    pub protocol: Option<String>,
    pub published_at: Option<String>,
    pub in_reply_to: Option<String>,
    pub atproto_uri: Option<String>,
    pub encrypted_message: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1User {
    pub actor_id: String,
    pub relation: String,
    pub status: String,
    pub created_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1TimelinePost {
    pub object_id: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub content: String,
    pub visibility: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub protocol: Option<String>,
    pub encrypted_message: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1FollowerRow {
    pub id: String,
    pub actor_id: String,
    pub follower_actor_id: String,
    pub follower_inbox: String,
    pub follower_shared_inbox: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1FollowingRow {
    pub id: String,
    pub actor_id: String,
    pub target_actor_id: String,
    pub target_inbox: String,
    pub status: String,
    pub created_at: Option<String>,
    pub accepted_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Notification {
    pub id: String,
    pub kind: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub post_id: Option<String>,
    pub activity_id: Option<String>,
    pub content: Option<String>,
    pub read: Option<bool>,
    pub created_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1DirectMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub content: String,
    pub published_at: String,
    pub created_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Block {
    pub id: String,
    pub actor_id: String,
    pub blocked_domain: Option<String>,
    pub reason: Option<String>,
    pub created_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Friend {
    pub friend_actor_id: String,
    pub friend_inbox: Option<String>,
    pub friend_shared_inbox: Option<String>,
    pub follower_since: Option<String>,
    pub following_since: Option<String>,
    pub accepted_at: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ServerStats {
    pub followers_total: u64,
    pub followers_approved: u64,
    pub followers_pending: u64,
    pub followers_rejected: u64,
    pub following_total: u64,
    pub posts_total: u64,
    pub activities_total: u64,
    pub deliveries_total: u64,
    pub deliveries_failed: u64,
    pub dual_protocol_posts: u64,
}

#[derive(Clone, Debug)]
pub struct D1Client {
    remote: bool,
    worker_dir: PathBuf,
}

impl D1Client {
    pub fn new(remote: bool) -> Result<Self> {
        let project_root = std::env::current_dir()?;
        Ok(Self {
            remote,
            worker_dir: project_root
                .join("platforms")
                .join("cloudflare")
                .join("workers")
                .join("actor"),
        })
    }

    pub async fn list_posts(&self, limit: u16) -> Result<Vec<D1Post>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, content, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, atproto_uri, encrypted_message, in_reply_to
            FROM posts
            ORDER BY published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn create_post(
        &self,
        id: &str,
        actor_id: &str,
        content: &str,
        visibility: &str,
        published_at: &str,
        in_reply_to: Option<&str>,
    ) -> Result<()> {
        let content_html = escape_html(content);
        let in_reply_to = in_reply_to
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO posts (
                id, actor_id, content, content_html, visibility,
                published_at, protocol, in_reply_to
            ) VALUES (
                {id}, {actor_id}, {content}, {content_html},
                {visibility}, {published_at}, 'activitypub', {in_reply_to}
            )
            "#,
            id = sql_literal(id),
            actor_id = sql_literal(actor_id),
            content = sql_literal(content),
            content_html = sql_literal(&content_html),
            visibility = sql_literal(visibility),
            published_at = sql_literal(published_at),
            in_reply_to = in_reply_to,
        );
        self.execute(&sql)
    }

    pub async fn create_encrypted_post(
        &self,
        id: &str,
        actor_id: &str,
        fallback_content: &str,
        visibility: &str,
        published_at: &str,
        encrypted_message_json: &str,
        in_reply_to: Option<&str>,
    ) -> Result<()> {
        let in_reply_to = in_reply_to
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO posts (
                id, actor_id, content, content_html, visibility,
                published_at, protocol, encrypted_message, in_reply_to
            ) VALUES (
                {id}, {actor_id}, {fallback_content}, {fallback_content},
                {visibility}, {published_at}, 'activitypub', {encrypted_message_json}, {in_reply_to}
            )
            "#,
            id = sql_literal(id),
            actor_id = sql_literal(actor_id),
            fallback_content = sql_literal(fallback_content),
            visibility = sql_literal(visibility),
            published_at = sql_literal(published_at),
            encrypted_message_json = sql_literal(encrypted_message_json),
            in_reply_to = in_reply_to,
        );
        self.execute(&sql)
    }

    pub async fn search_posts(&self, needle: &str, limit: u16) -> Result<Vec<D1Post>> {
        let limit = clamp_limit(limit);
        let needle = sql_like_escape(needle);
        let sql = format!(
            r#"
            SELECT id, content, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, atproto_uri, encrypted_message, in_reply_to
            FROM posts
            WHERE content LIKE '%{needle}%'
            ORDER BY published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn search_users(&self, needle: &str, limit: u16) -> Result<Vec<D1User>> {
        let limit = clamp_limit(limit);
        let needle = sql_like_escape(needle);
        let sql = format!(
            r#"
            SELECT follower_actor_id AS actor_id, 'follower' AS relation, status, created_at
            FROM followers
            WHERE follower_actor_id LIKE '%{needle}%'
            UNION ALL
            SELECT target_actor_id AS actor_id, 'following' AS relation, status, created_at
            FROM following
            WHERE target_actor_id LIKE '%{needle}%'
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn home_timeline(
        &self,
        limit: u16,
        before: Option<&str>,
    ) -> Result<Vec<D1TimelinePost>> {
        let limit = clamp_limit(limit);
        let before_filter = before
            .map(sql_literal)
            .map(|value| format!("AND published_at < {value}"))
            .unwrap_or_default();
        let sql = format!(
            r#"
            SELECT object_id, actor_id, actor_username, actor_display_name,
                   content, visibility, published_at, updated_at, protocol, encrypted_message
            FROM timeline_posts
            WHERE deleted_at IS NULL {before_filter}
            ORDER BY published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn list_followers(&self, limit: u16) -> Result<Vec<D1FollowerRow>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox,
                   status, created_at, updated_at
            FROM followers
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn create_follower_deliveries(
        &self,
        post_id: &str,
        _actor_id: &str,
        _activity_json: &str,
    ) -> Result<Vec<String>> {
        let followers = self.list_followers(500).await?;
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut delivery_ids = Vec::new();

        for follower in followers.into_iter().filter(|row| row.status == "approved") {
            let delivery_id = format!("delivery-{}", uuid_like());
            let sql = format!(
                r#"
                INSERT INTO deliveries (
                    id, post_id, target_type, target_url, protocol,
                    status, retry_count, created_at
                ) VALUES (
                    {id}, {post_id}, 'inbox', {target_url}, 'activitypub',
                    'queued', 0, {created_at}
                )
                "#,
                id = sql_literal(&delivery_id),
                post_id = sql_literal(post_id),
                target_url = sql_literal(&follower.follower_inbox),
                created_at = sql_literal(&created_at),
            );
            self.execute(&sql)?;
            delivery_ids.push(delivery_id);
        }

        Ok(delivery_ids)
    }

    pub async fn create_direct_deliveries(
        &self,
        post_id: &str,
        _actor_id: &str,
        _activity_json: &str,
        recipients: &[String],
    ) -> Result<Vec<String>> {
        let followers = self.list_followers(500).await?;
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut delivery_ids = Vec::new();
        let mut missing = Vec::new();
        let approved: Vec<&D1FollowerRow> = recipients
            .iter()
            .filter_map(|recipient| {
                followers
                    .iter()
                    .find(|row| row.status == "approved" && row.follower_actor_id == *recipient)
            })
            .collect();

        for recipient in recipients {
            if followers
                .iter()
                .any(|row| row.status == "approved" && row.follower_actor_id == *recipient)
            {
                continue;
            }

            missing.push(recipient.clone());
        }

        if !missing.is_empty() {
            anyhow::bail!(
                "direct recipients must be approved followers with known inboxes: {}",
                missing.join(", ")
            );
        }

        for follower in approved {
            let delivery_id = format!("delivery-{}", uuid_like());
            let sql = format!(
                r#"
                INSERT INTO deliveries (
                    id, post_id, target_type, target_url, protocol,
                    status, retry_count, created_at
                ) VALUES (
                    {id}, {post_id}, 'inbox', {target_url}, 'activitypub',
                    'queued', 0, {created_at}
                )
                "#,
                id = sql_literal(&delivery_id),
                post_id = sql_literal(post_id),
                target_url = sql_literal(&follower.follower_inbox),
                created_at = sql_literal(&created_at),
            );
            self.execute(&sql)?;
            delivery_ids.push(delivery_id);
        }

        Ok(delivery_ids)
    }

    pub async fn list_following(&self, limit: u16) -> Result<Vec<D1FollowingRow>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, actor_id, target_actor_id, target_inbox,
                   status, created_at, accepted_at
            FROM following
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn list_notifications(&self, limit: u16) -> Result<Vec<D1Notification>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, type AS kind, actor_id, actor_username, actor_display_name,
                   actor_avatar_url, post_id, activity_id, content, read, created_at
            FROM notifications
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn list_direct_messages(&self, limit: u16) -> Result<Vec<D1DirectMessage>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, conversation_id, sender_id, content, published_at, created_at
            FROM direct_messages
            ORDER BY published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn list_blocks(&self, limit: u16) -> Result<Vec<D1Block>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, actor_id, blocked_domain, reason, created_at
            FROM blocks
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn approve_follower(&self, actor_id: &str, follower_actor_id: &str) -> Result<()> {
        let sql = format!(
            "UPDATE followers SET status = 'approved', updated_at = CURRENT_TIMESTAMP WHERE actor_id = {actor_id} AND follower_actor_id = {follower_actor_id}",
            actor_id = sql_literal(actor_id),
            follower_actor_id = sql_literal(follower_actor_id),
        );
        self.execute(&sql)
    }

    pub async fn reject_follower(&self, actor_id: &str, follower_actor_id: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM followers WHERE actor_id = {actor_id} AND follower_actor_id = {follower_actor_id}",
            actor_id = sql_literal(actor_id),
            follower_actor_id = sql_literal(follower_actor_id),
        );
        self.execute(&sql)
    }

    pub async fn mark_notification_read(&self, id: &str) -> Result<()> {
        let sql = format!(
            "UPDATE notifications SET read = 1 WHERE id = {id}",
            id = sql_literal(id)
        );
        self.execute(&sql)
    }

    #[allow(dead_code)]
    pub async fn block_actor(&self, actor_id: &str, reason: Option<&str>) -> Result<()> {
        let reason_sql = reason
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT OR REPLACE INTO blocks (id, actor_id, reason, created_at)
            VALUES ({id}, {actor_id}, {reason}, CURRENT_TIMESTAMP)
            "#,
            id = sql_literal(actor_id),
            actor_id = sql_literal(actor_id),
            reason = reason_sql,
        );
        self.execute(&sql)
    }

    pub async fn unblock_actor(&self, actor_id: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM blocks WHERE actor_id = {actor_id}",
            actor_id = sql_literal(actor_id)
        );
        self.execute(&sql)
    }

    pub async fn list_friends(&self, actor: &str, limit: u16) -> Result<Vec<D1Friend>> {
        let limit = clamp_limit(limit);
        let actor = sql_literal(actor);
        let sql = format!(
            r#"
            SELECT friend_actor_id, friend_inbox, friend_shared_inbox,
                   follower_since, following_since, accepted_at
            FROM friends
            WHERE local_actor_id = {actor}
            ORDER BY COALESCE(accepted_at, following_since, follower_since) DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn stats(&self) -> Result<ServerStats> {
        let row: Value = self
            .query_one(
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
                    (SELECT COUNT(*) FROM posts WHERE protocol='both') AS dual_protocol_posts
                "#,
            )?
            .unwrap_or(Value::Null);

        Ok(ServerStats {
            followers_total: value_u64(&row, "followers_total"),
            followers_approved: value_u64(&row, "followers_approved"),
            followers_pending: value_u64(&row, "followers_pending"),
            followers_rejected: value_u64(&row, "followers_rejected"),
            following_total: value_u64(&row, "following_total"),
            posts_total: value_u64(&row, "posts_total"),
            activities_total: value_u64(&row, "activities_total"),
            deliveries_total: value_u64(&row, "deliveries_total"),
            deliveries_failed: value_u64(&row, "deliveries_failed"),
            dual_protocol_posts: value_u64(&row, "dual_protocol_posts"),
        })
    }

    fn query<T: for<'de> Deserialize<'de>>(&self, sql: &str) -> Result<Vec<T>> {
        let rows = self.query_values(sql)?;
        rows.into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("could not decode D1 rows")
    }

    fn query_one(&self, sql: &str) -> Result<Option<Value>> {
        Ok(self.query_values(sql)?.into_iter().next())
    }

    fn query_values(&self, sql: &str) -> Result<Vec<Value>> {
        let mut command = Command::new(wrangler_bin()?);
        command
            .args(["d1", "execute", "dais-social", "--command", sql])
            .current_dir(&self.worker_dir);
        if self.remote {
            command.arg("--remote");
        } else {
            command.arg("--local");
        }

        let output = command.output().map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                anyhow!(
                    "wrangler is not installed or is not on PATH; required for D1-backed client commands"
                )
            } else if !self.worker_dir.exists() {
                anyhow!("worker directory does not exist: {}", self.worker_dir.display())
            } else {
                anyhow!(
                    "failed to run wrangler from {}: {error}",
                    self.worker_dir.display()
                )
            }
        })?;

        if !output.status.success() {
            return Err(anyhow!(
                "wrangler d1 execute failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        parse_wrangler_results(&String::from_utf8_lossy(&output.stdout))
    }

    fn execute(&self, sql: &str) -> Result<()> {
        self.query_values(sql).map(|_| ())
    }
}

fn parse_wrangler_results(output: &str) -> Result<Vec<Value>> {
    let start = output
        .find('[')
        .ok_or_else(|| anyhow!("wrangler output did not contain JSON results"))?;
    let end = output
        .rfind(']')
        .ok_or_else(|| anyhow!("wrangler output did not contain JSON results"))?
        + 1;
    let batches: Vec<Value> = serde_json::from_str(&output[start..end])
        .with_context(|| format!("could not parse wrangler output: {output}"))?;

    let results = batches
        .first()
        .and_then(|batch| batch.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(results)
}

fn value_u64(row: &Value, key: &str) -> u64 {
    row.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn clamp_limit(limit: u16) -> u16 {
    limit.clamp(1, 200)
}

fn sql_like_escape(value: &str) -> String {
    value.replace('\'', "''")
}

fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn uuid_like() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn wrangler_bin() -> Result<PathBuf> {
    let local = std::env::current_dir()?
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) {
            "wrangler.cmd"
        } else {
            "wrangler"
        });
    if local.exists() {
        return Ok(local);
    }

    Ok(Path::new("wrangler").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::parse_wrangler_results;

    #[test]
    fn parses_wrangler_json_results_from_noisy_output() {
        let output = r#"
        noise
        [
          {"results":[{"count":2}],"success":true}
        ]
        "#;
        let rows = parse_wrangler_results(output).unwrap();
        assert_eq!(rows[0]["count"], 2);
    }
}
