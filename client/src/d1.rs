use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Post {
    pub id: String,
    pub content: String,
    pub object_type: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
    pub poll_options: Option<String>,
    pub visibility: Option<String>,
    pub protocol: Option<String>,
    pub published_at: Option<String>,
    pub in_reply_to: Option<String>,
    pub atproto_uri: Option<String>,
    pub encrypted_message: Option<String>,
    pub media_attachments: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Actor {
    pub id: String,
    pub username: String,
    pub actor_type: Option<String>,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub icon: Option<String>,
    pub image: Option<String>,
}

pub struct EncryptedPostInsert<'a> {
    pub id: &'a str,
    pub actor_id: &'a str,
    pub fallback_content: &'a str,
    pub visibility: &'a str,
    pub published_at: &'a str,
    pub encrypted_message_json: &'a str,
    pub in_reply_to: Option<&'a str>,
    pub media_attachments: Option<&'a str>,
}

pub struct ActivityDeliveryInsert<'a> {
    pub post_id: &'a str,
    pub actor_id: &'a str,
    pub activity_type: &'a str,
    pub activity_json: &'a str,
    pub target_inboxes: &'a [String],
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
    #[serde(default, deserialize_with = "optional_bool_from_d1")]
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
pub struct D1AllowlistHost {
    pub host: String,
    pub note: Option<String>,
    pub enabled: Option<u64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1ActivityRow {
    pub id: String,
    pub kind: String,
    pub actor: Option<String>,
    pub object: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1TopPost {
    pub post_id: String,
    pub content: String,
    pub visibility: Option<String>,
    pub published_at: Option<String>,
    pub replies: u64,
    pub likes: u64,
    pub boosts: u64,
    pub total: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1SourceSubscription {
    pub id: String,
    pub source_type: String,
    pub url: String,
    pub title: Option<String>,
    pub homepage_url: Option<String>,
    pub status: String,
    pub refresh_cadence_minutes: u64,
    pub last_fetched_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_error: Option<String>,
    pub error_count: u64,
    pub policy_json: String,
    pub api_secret_name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1SourceItem {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub canonical_url: Option<String>,
    pub external_id: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub fetched_at: Option<String>,
    pub excerpt: Option<String>,
    pub content_type: Option<String>,
    pub hash: String,
    pub thumbnail_url: Option<String>,
    pub rights_policy_json: String,
    pub read: Option<u64>,
    pub summary: Option<String>,
    pub raw_metadata_json: Option<String>,
}

pub struct SourceSubscriptionInsert<'a> {
    pub id: &'a str,
    pub source_type: &'a str,
    pub url: &'a str,
    pub title: Option<&'a str>,
    pub cadence_minutes: u16,
    pub policy_json: &'a str,
    pub api_secret_name: Option<&'a str>,
}

pub struct SourceItemInsert<'a> {
    pub id: &'a str,
    pub source_id: &'a str,
    pub source_type: &'a str,
    pub title: &'a str,
    pub canonical_url: Option<&'a str>,
    pub external_id: Option<&'a str>,
    pub author: Option<&'a str>,
    pub published_at: Option<&'a str>,
    pub excerpt: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub hash: &'a str,
    pub thumbnail_url: Option<&'a str>,
    pub rights_policy_json: &'a str,
    pub raw_metadata_json: Option<&'a str>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct D1Delivery {
    pub id: String,
    pub post_id: String,
    pub target_url: String,
    pub protocol: String,
    pub status: String,
    pub retry_count: Option<u64>,
    pub error_message: Option<String>,
    pub created_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub delivered_at: Option<String>,
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
    pub deliveries_queued: u64,
    pub deliveries_retry: u64,
    pub deliveries_delivered: u64,
    pub dual_protocol_posts: u64,
    pub public_posts: u64,
    pub private_posts: u64,
    pub direct_posts: u64,
    pub encrypted_posts: u64,
    pub media_posts: u64,
    pub notifications_unread: u64,
    pub blocks_total: u64,
    pub allowlist_hosts: u64,
    pub closed_network: bool,
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
            SELECT id, content, COALESCE(object_type, 'Note') AS object_type, name, summary,
                   start_time, end_time, location, poll_options,
                   visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, atproto_uri, encrypted_message, in_reply_to, media_attachments
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
        object_type: crate::cli::ActivityObjectType,
        name: Option<&str>,
        summary: Option<&str>,
        starts_at: Option<&str>,
        ends_at: Option<&str>,
        location: Option<&str>,
        poll_options_json: Option<&str>,
        media_attachments_json: Option<&str>,
    ) -> Result<()> {
        let content_html = escape_html(content);
        let in_reply_to = in_reply_to
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let name = name.map(sql_literal).unwrap_or_else(|| "NULL".to_string());
        let summary = summary
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let starts_at = starts_at
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let ends_at = ends_at
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let location = location
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let media_attachments = media_attachments_json
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let poll_options = poll_options_json
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO posts (
                id, actor_id, content, content_html, object_type, name, summary, visibility,
                published_at, protocol, in_reply_to, start_time, end_time, location,
                poll_options, media_attachments
            ) VALUES (
                {id}, {actor_id}, {content}, {content_html}, {object_type}, {name}, {summary},
                {visibility}, {published_at}, 'activitypub', {in_reply_to}, {starts_at}, {ends_at},
                {location}, {poll_options}, {media_attachments}
            )
            "#,
            id = sql_literal(id),
            actor_id = sql_literal(actor_id),
            content = sql_literal(content),
            content_html = sql_literal(&content_html),
            object_type = sql_literal(&object_type.to_string()),
            name = name,
            summary = summary,
            visibility = sql_literal(visibility),
            published_at = sql_literal(published_at),
            in_reply_to = in_reply_to,
            starts_at = starts_at,
            ends_at = ends_at,
            location = location,
            poll_options = poll_options,
            media_attachments = media_attachments,
        );
        self.execute(&sql)
    }

    pub async fn create_encrypted_post(&self, post: EncryptedPostInsert<'_>) -> Result<()> {
        let in_reply_to = post
            .in_reply_to
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let media_attachments = post
            .media_attachments
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO posts (
                id, actor_id, content, content_html, visibility,
                published_at, protocol, encrypted_message, in_reply_to, media_attachments
            ) VALUES (
                {id}, {actor_id}, {fallback_content}, {fallback_content},
                {visibility}, {published_at}, 'activitypub', {encrypted_message_json}, {in_reply_to}, {media_attachments}
            )
            "#,
            id = sql_literal(post.id),
            actor_id = sql_literal(post.actor_id),
            fallback_content = sql_literal(post.fallback_content),
            visibility = sql_literal(post.visibility),
            published_at = sql_literal(post.published_at),
            encrypted_message_json = sql_literal(post.encrypted_message_json),
            in_reply_to = in_reply_to,
            media_attachments = media_attachments,
        );
        self.execute(&sql)
    }

    pub async fn search_posts(&self, needle: &str, limit: u16) -> Result<Vec<D1Post>> {
        let limit = clamp_limit(limit);
        let needle = sql_like_escape(needle);
        let sql = format!(
            r#"
            SELECT id, content, COALESCE(object_type, 'Note') AS object_type, name, summary,
                   start_time, end_time, location, poll_options,
                   visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, atproto_uri, encrypted_message, in_reply_to, media_attachments
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
            let target_url = follower
                .follower_shared_inbox
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or(&follower.follower_inbox);
            if !self.is_federation_target_allowed(target_url)? {
                continue;
            }
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
                target_url = sql_literal(target_url),
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
            if !self.is_federation_target_allowed(&follower.follower_inbox)? {
                continue;
            }
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

    pub async fn create_activity_deliveries(
        &self,
        input: ActivityDeliveryInsert<'_>,
    ) -> Result<Vec<String>> {
        let mut targets: Vec<String> = input
            .target_inboxes
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect();

        if targets.is_empty() {
            targets = self
                .list_followers(500)
                .await?
                .into_iter()
                .filter(|row| row.status == "approved")
                .map(|row| {
                    row.follower_shared_inbox
                        .filter(|value| !value.is_empty())
                        .unwrap_or(row.follower_inbox)
                })
                .collect();
        }

        targets.sort();
        targets.dedup();
        targets.retain(|target| self.is_federation_target_allowed(target).unwrap_or(false));

        let created_at = chrono::Utc::now().to_rfc3339();
        let mut delivery_ids = Vec::new();
        for target_url in targets {
            let delivery_id = format!("delivery-{}", uuid_like());
            let sql = format!(
                r#"
                INSERT INTO deliveries (
                    id, post_id, target_type, target_url, protocol,
                    status, retry_count, created_at, activity_type, activity_json
                ) VALUES (
                    {id}, {post_id}, 'inbox', {target_url}, 'activitypub',
                    'queued', 0, {created_at}, {activity_type}, {activity_json}
                )
                "#,
                id = sql_literal(&delivery_id),
                post_id = sql_literal(input.post_id),
                target_url = sql_literal(&target_url),
                created_at = sql_literal(&created_at),
                activity_type = sql_literal(input.activity_type),
                activity_json = sql_literal(input.activity_json),
            );
            self.execute(&sql)?;
            delivery_ids.push(delivery_id);
        }

        let _ = input.actor_id;
        Ok(delivery_ids)
    }

    pub async fn update_post_content(&self, post_id: &str, content: &str) -> Result<()> {
        let content_html = escape_html(content);
        let sql = format!(
            r#"
            UPDATE posts
            SET content = {content}, content_html = {content_html}, updated_at = CURRENT_TIMESTAMP
            WHERE id = {post_id}
            "#,
            content = sql_literal(content),
            content_html = sql_literal(&content_html),
            post_id = sql_literal(post_id),
        );
        self.execute(&sql)
    }

    pub async fn delete_post(&self, post_id: &str) -> Result<()> {
        let sql = format!("DELETE FROM posts WHERE id = {}", sql_literal(post_id));
        self.execute(&sql)
    }

    pub async fn record_interaction(
        &self,
        activity_id: &str,
        interaction_type: &str,
        actor_id: &str,
        object_id: &str,
    ) -> Result<()> {
        let sql = format!(
            r#"
            INSERT OR REPLACE INTO interactions (
                id, type, actor_id, object_url, created_at
            ) VALUES (
                {id}, {interaction_type}, {actor_id}, {object_id}, {created_at}
            )
            "#,
            id = sql_literal(activity_id),
            interaction_type = sql_literal(interaction_type),
            actor_id = sql_literal(actor_id),
            object_id = sql_literal(object_id),
            created_at = sql_literal(&chrono::Utc::now().to_rfc3339()),
        );
        self.execute(&sql)
    }

    pub async fn remove_interaction(&self, activity_id: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM interactions WHERE id = {}",
            sql_literal(activity_id)
        );
        self.execute(&sql)
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

    pub async fn block_actor(&self, actor_id: &str, reason: Option<&str>) -> Result<()> {
        let reason_sql = reason
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT OR REPLACE INTO blocks (id, actor_id, blocked_domain, reason, created_at)
            VALUES ({id}, {actor_id}, NULL, {reason}, CURRENT_TIMESTAMP)
            "#,
            id = sql_literal(actor_id),
            actor_id = sql_literal(actor_id),
            reason = reason_sql,
        );
        self.execute(&sql)
    }

    pub async fn block_domain(&self, domain: &str, reason: Option<&str>) -> Result<()> {
        let normalized = normalize_host(domain);
        let reason_sql = reason
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT OR REPLACE INTO blocks (id, actor_id, blocked_domain, reason, created_at)
            VALUES ({id}, {actor_id}, {blocked_domain}, {reason}, CURRENT_TIMESTAMP)
            "#,
            id = sql_literal(&normalized),
            actor_id = sql_literal(&normalized),
            blocked_domain = sql_literal(&normalized),
            reason = reason_sql,
        );
        self.execute(&sql)
    }

    pub async fn unblock(&self, value: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM blocks WHERE actor_id = {value} OR blocked_domain = {value}",
            value = sql_literal(value)
        );
        self.execute(&sql)
    }

    pub async fn list_deliveries(
        &self,
        limit: u16,
        status: Option<&str>,
    ) -> Result<Vec<D1Delivery>> {
        let limit = clamp_limit(limit);
        let status_filter = match status {
            Some(value) => {
                let value = normalized_delivery_status(value)?;
                format!("WHERE status = {}", sql_literal(value))
            }
            None => String::new(),
        };
        let sql = format!(
            r#"
            SELECT id, post_id, target_url, protocol, status, retry_count,
                   error_message, created_at, last_attempt_at, delivered_at
            FROM deliveries
            {status_filter}
            ORDER BY COALESCE(last_attempt_at, created_at) DESC
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

    pub async fn get_actor(&self, username: &str) -> Result<Option<D1Actor>> {
        let sql = format!(
            r#"
            SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type,
                   display_name, summary, icon, image
            FROM actors
            WHERE username = {username}
            "#,
            username = sql_literal(username),
        );
        self.query_one(&sql)?
            .map(serde_json::from_value)
            .transpose()
            .context("could not decode actor row")
    }

    pub async fn update_actor_profile(
        &self,
        username: &str,
        display_name: Option<&str>,
        summary: Option<&str>,
        icon: Option<&str>,
        image: Option<&str>,
    ) -> Result<()> {
        let mut assignments = vec!["updated_at = CURRENT_TIMESTAMP".to_string()];
        if let Some(value) = display_name {
            assignments.push(format!("display_name = {}", sql_literal(value)));
        }
        if let Some(value) = summary {
            assignments.push(format!("summary = {}", sql_literal(value)));
        }
        if let Some(value) = icon {
            assignments.push(format!("icon = {}", sql_literal(value)));
            assignments.push(format!("avatar_url = {}", sql_literal(value)));
        }
        if let Some(value) = image {
            assignments.push(format!("image = {}", sql_literal(value)));
            assignments.push(format!("header_url = {}", sql_literal(value)));
        }
        if assignments.len() == 1 {
            anyhow::bail!("no profile fields provided");
        }
        let sql = format!(
            "UPDATE actors SET {} WHERE username = {}",
            assignments.join(", "),
            sql_literal(username)
        );
        self.execute(&sql)
    }

    pub async fn list_allowlist_hosts(&self) -> Result<Vec<D1AllowlistHost>> {
        self.query(
            r#"
            SELECT host, note, enabled, created_at, updated_at
            FROM federation_allowlist
            ORDER BY host ASC
            "#,
        )
    }

    pub async fn closed_network_enabled(&self) -> Result<bool> {
        self.is_closed_network_enabled()
    }

    pub async fn set_closed_network(&self, enabled: bool) -> Result<()> {
        let enabled = if enabled { 1 } else { 0 };
        let sql = format!("UPDATE instance_settings SET closed_network = {enabled} WHERE id = 1");
        self.execute(&sql)
    }

    pub async fn allow_federation_host(&self, host: &str, note: Option<&str>) -> Result<()> {
        let host = normalize_host(host);
        let note = note.map(sql_literal).unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO federation_allowlist (host, note, enabled, created_at, updated_at)
            VALUES ({host}, {note}, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(host) DO UPDATE SET
                note = excluded.note,
                enabled = 1,
                updated_at = CURRENT_TIMESTAMP
            "#,
            host = sql_literal(&host),
            note = note,
        );
        self.execute(&sql)
    }

    pub async fn disallow_federation_host(&self, host: &str) -> Result<()> {
        let host = normalize_host(host);
        let sql = format!(
            "DELETE FROM federation_allowlist WHERE host = {}",
            sql_literal(&host)
        );
        self.execute(&sql)
    }

    pub async fn set_actor_type(
        &self,
        username: &str,
        actor_type: crate::cli::ActorType,
    ) -> Result<()> {
        let sql = format!(
            r#"
            UPDATE actors
            SET actor_type = {actor_type}
            WHERE username = {username}
            "#,
            actor_type = sql_literal(&actor_type.to_string()),
            username = sql_literal(username),
        );
        self.execute(&sql)
    }

    pub async fn list_events(&self, limit: u16) -> Result<Vec<D1Post>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, content, COALESCE(object_type, 'Note') AS object_type, name, summary,
                   start_time, end_time, location, poll_options, visibility,
                   COALESCE(protocol, 'activitypub') AS protocol, published_at, atproto_uri,
                   encrypted_message, in_reply_to, media_attachments
            FROM posts
            WHERE COALESCE(object_type, 'Note') = 'Event'
            ORDER BY COALESCE(start_time, published_at) DESC
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
            deliveries_queued: value_u64(&row, "deliveries_queued"),
            deliveries_retry: value_u64(&row, "deliveries_retry"),
            deliveries_delivered: value_u64(&row, "deliveries_delivered"),
            dual_protocol_posts: value_u64(&row, "dual_protocol_posts"),
            public_posts: value_u64(&row, "public_posts"),
            private_posts: value_u64(&row, "private_posts"),
            direct_posts: value_u64(&row, "direct_posts"),
            encrypted_posts: value_u64(&row, "encrypted_posts"),
            media_posts: value_u64(&row, "media_posts"),
            notifications_unread: value_u64(&row, "notifications_unread"),
            blocks_total: value_u64(&row, "blocks_total"),
            allowlist_hosts: value_u64(&row, "allowlist_hosts"),
            closed_network: row
                .get("closed_network")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                == 1,
        })
    }

    pub async fn activity_report(&self, limit: u16) -> Result<Vec<D1ActivityRow>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, type AS kind, actor, object, NULL AS status, received_at AS created_at
            FROM activities
            UNION ALL
            SELECT id, 'delivery' AS kind, target_url AS actor, post_id AS object, status, created_at
            FROM deliveries
            ORDER BY created_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn top_posts(&self, limit: u16) -> Result<Vec<D1TopPost>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT
                p.id AS post_id,
                p.content,
                p.visibility,
                p.published_at,
                (SELECT COUNT(*) FROM replies r WHERE r.post_id = p.id) AS replies,
                (SELECT COUNT(*) FROM interactions i WHERE i.post_id = p.id AND i.type = 'like') AS likes,
                (SELECT COUNT(*) FROM interactions i WHERE i.post_id = p.id AND i.type = 'boost') AS boosts,
                (
                    (SELECT COUNT(*) FROM replies r WHERE r.post_id = p.id) +
                    (SELECT COUNT(*) FROM interactions i WHERE i.post_id = p.id)
                ) AS total
            FROM posts p
            ORDER BY total DESC, p.published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn add_source_subscription(
        &self,
        source: SourceSubscriptionInsert<'_>,
    ) -> Result<()> {
        let title = source
            .title
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let api_secret_name = source
            .api_secret_name
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT INTO source_subscriptions (
                id, source_type, url, title, refresh_cadence_minutes, policy_json,
                api_secret_name, next_fetch_at, created_at, updated_at
            ) VALUES (
                {id}, {source_type}, {url}, {title}, {cadence}, {policy_json},
                {api_secret_name}, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
            )
            ON CONFLICT(url) DO UPDATE SET
                source_type = excluded.source_type,
                title = excluded.title,
                refresh_cadence_minutes = excluded.refresh_cadence_minutes,
                policy_json = excluded.policy_json,
                api_secret_name = excluded.api_secret_name,
                status = 'active',
                updated_at = CURRENT_TIMESTAMP
            "#,
            id = sql_literal(source.id),
            source_type = sql_literal(source.source_type),
            url = sql_literal(source.url),
            title = title,
            cadence = source.cadence_minutes,
            policy_json = sql_literal(source.policy_json),
            api_secret_name = api_secret_name,
        );
        self.execute(&sql)
    }

    pub async fn list_sources(&self, limit: u16) -> Result<Vec<D1SourceSubscription>> {
        let limit = clamp_limit(limit);
        let sql = format!(
            r#"
            SELECT id, source_type, url, title, homepage_url, status,
                   refresh_cadence_minutes, last_fetched_at, next_fetch_at, etag,
                   last_modified, last_error, error_count, policy_json,
                   api_secret_name, created_at, updated_at
            FROM source_subscriptions
            ORDER BY updated_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn get_source(&self, id: &str) -> Result<Option<D1SourceSubscription>> {
        let sql = format!(
            r#"
            SELECT id, source_type, url, title, homepage_url, status,
                   refresh_cadence_minutes, last_fetched_at, next_fetch_at, etag,
                   last_modified, last_error, error_count, policy_json,
                   api_secret_name, created_at, updated_at
            FROM source_subscriptions
            WHERE id = {id}
            "#,
            id = sql_literal(id)
        );
        Ok(self.query(&sql)?.into_iter().next())
    }

    pub async fn active_sources(&self) -> Result<Vec<D1SourceSubscription>> {
        self.query(
            r#"
            SELECT id, source_type, url, title, homepage_url, status,
                   refresh_cadence_minutes, last_fetched_at, next_fetch_at, etag,
                   last_modified, last_error, error_count, policy_json,
                   api_secret_name, created_at, updated_at
            FROM source_subscriptions
            WHERE status = 'active'
            ORDER BY updated_at DESC
            LIMIT 200
            "#,
        )
    }

    pub async fn remove_source(&self, id: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM source_subscriptions WHERE id = {}",
            sql_literal(id)
        );
        self.execute(&sql)
    }

    pub async fn upsert_source_item(&self, item: SourceItemInsert<'_>) -> Result<()> {
        let canonical_url = item
            .canonical_url
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let external_id = item
            .external_id
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let author = item
            .author
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let published_at = item
            .published_at
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let excerpt = item
            .excerpt
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let content_type = item
            .content_type
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let thumbnail_url = item
            .thumbnail_url
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let raw_metadata_json = item
            .raw_metadata_json
            .map(sql_literal)
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            r#"
            INSERT OR IGNORE INTO source_items (
                id, source_id, source_type, title, canonical_url, external_id, author,
                published_at, excerpt, content_type, hash, thumbnail_url,
                rights_policy_json, raw_metadata_json, fetched_at, created_at, updated_at
            ) VALUES (
                {id}, {source_id}, {source_type}, {title}, {canonical_url}, {external_id}, {author},
                {published_at}, {excerpt}, {content_type}, {hash}, {thumbnail_url},
                {rights_policy_json}, {raw_metadata_json}, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            )
            "#,
            id = sql_literal(item.id),
            source_id = sql_literal(item.source_id),
            source_type = sql_literal(item.source_type),
            title = sql_literal(item.title),
            canonical_url = canonical_url,
            external_id = external_id,
            author = author,
            published_at = published_at,
            excerpt = excerpt,
            content_type = content_type,
            hash = sql_literal(item.hash),
            thumbnail_url = thumbnail_url,
            rights_policy_json = sql_literal(item.rights_policy_json),
            raw_metadata_json = raw_metadata_json,
        );
        self.execute(&sql)
    }

    pub async fn mark_source_refreshed(
        &self,
        id: &str,
        cadence_minutes: u64,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<()> {
        let next_fetch = chrono::Utc::now() + chrono::Duration::minutes(cadence_minutes as i64);
        let etag = etag.map(sql_literal).unwrap_or_else(|| "etag".to_string());
        let last_modified = last_modified
            .map(sql_literal)
            .unwrap_or_else(|| "last_modified".to_string());
        let sql = format!(
            r#"
            UPDATE source_subscriptions
            SET status = 'active',
                last_fetched_at = CURRENT_TIMESTAMP,
                next_fetch_at = {next_fetch},
                etag = COALESCE({etag}, etag),
                last_modified = COALESCE({last_modified}, last_modified),
                last_error = NULL,
                error_count = 0,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = {id}
            "#,
            next_fetch = sql_literal(&next_fetch.to_rfc3339()),
            etag = etag,
            last_modified = last_modified,
            id = sql_literal(id),
        );
        self.execute(&sql)
    }

    pub async fn mark_source_error(&self, id: &str, error: &str) -> Result<()> {
        let sql = format!(
            r#"
            UPDATE source_subscriptions
            SET status = 'error',
                last_error = {error},
                error_count = error_count + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = {id}
            "#,
            error = sql_literal(error),
            id = sql_literal(id),
        );
        self.execute(&sql)
    }

    pub async fn list_source_items(
        &self,
        source_id: Option<&str>,
        limit: u16,
        unread: bool,
    ) -> Result<Vec<D1SourceItem>> {
        let limit = clamp_limit(limit);
        let mut filters = Vec::new();
        if let Some(source_id) = source_id {
            filters.push(format!("source_id = {}", sql_literal(source_id)));
        }
        if unread {
            filters.push("read = 0".to_string());
        }
        let where_clause = if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", filters.join(" AND "))
        };
        let sql = format!(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, hash, thumbnail_url,
                   rights_policy_json, read, summary, raw_metadata_json
            FROM source_items
            {where_clause}
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub fn upload_media(
        &self,
        bucket: &str,
        key: &str,
        path: &Path,
        public_base_url: &str,
    ) -> Result<String> {
        let mut command = Command::new(wrangler_bin()?);
        command
            .args(["r2", "object", "put", &format!("{bucket}/{key}")])
            .arg("--file")
            .arg(path)
            .current_dir(&self.worker_dir);
        if self.remote {
            command.arg("--remote");
        } else {
            command.arg("--local");
        }

        let output = command
            .output()
            .context("failed to run wrangler r2 object put")?;
        if !output.status.success() {
            anyhow::bail!(
                "wrangler r2 object put failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(format!(
            "{}/{}",
            public_base_url.trim_end_matches('/'),
            key.trim_start_matches('/')
        ))
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

    fn is_closed_network_enabled(&self) -> Result<bool> {
        let row = match self.query_one("SELECT closed_network FROM instance_settings WHERE id = 1")
        {
            Ok(row) => row,
            Err(_) => return Ok(false),
        };
        Ok(row
            .as_ref()
            .and_then(|value| value.get("closed_network"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            == 1)
    }

    fn is_federation_target_allowed(&self, target_url: &str) -> Result<bool> {
        if !self.is_closed_network_enabled()? {
            return Ok(true);
        }

        let Some(host) = https_host(target_url) else {
            return Ok(false);
        };
        let sql = format!(
            r#"
            SELECT COUNT(*) AS count
            FROM federation_allowlist
            WHERE host = {host}
              AND enabled = 1
            "#,
            host = sql_literal(host),
        );
        let count = self
            .query_one(&sql)?
            .as_ref()
            .and_then(|row| row.get("count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(count > 0)
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

fn optional_bool_from_d1<'de, D>(deserializer: D) -> std::result::Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(Value::Number(value)) => match value.as_u64() {
            Some(0) => Ok(Some(false)),
            Some(1) => Ok(Some(true)),
            _ => Err(serde::de::Error::custom(
                "expected D1 boolean integer 0 or 1",
            )),
        },
        Some(Value::String(value)) => match value.as_str() {
            "0" => Ok(Some(false)),
            "1" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            "true" => Ok(Some(true)),
            _ => Err(serde::de::Error::custom("expected D1 boolean string")),
        },
        Some(_) => Err(serde::de::Error::custom("expected D1 boolean value")),
    }
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

fn https_host(value: &str) -> Option<&str> {
    let rest = value.strip_prefix("https://")?;
    rest.split('/').next().filter(|host| !host.is_empty())
}

fn normalize_host(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    trimmed
        .split('/')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

fn normalized_delivery_status(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "queued" => Ok("queued"),
        "retry" => Ok("retry"),
        "failed" => Ok("failed"),
        "delivered" => Ok("delivered"),
        other => Err(anyhow!(
            "unsupported delivery status {other}; expected queued, retry, failed, or delivered"
        )),
    }
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
    if let Ok(path) = std::env::var("WRANGLER") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    Ok(Path::new("wrangler").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{https_host, normalize_host, parse_wrangler_results, D1Notification};

    #[test]
    fn notification_read_decodes_d1_integer_boolean() {
        let row = serde_json::json!({
            "id": "notification-1",
            "kind": "like",
            "actor_id": "https://mastodon.example/users/alice",
            "read": 0
        });

        let notification: D1Notification = serde_json::from_value(row).unwrap();
        assert_eq!(notification.read, Some(false));
    }

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

    #[test]
    fn extracts_https_host_for_allowlist_checks() {
        assert_eq!(
            https_host("https://mastodon.social/inbox"),
            Some("mastodon.social")
        );
        assert_eq!(
            https_host("https://social.example/users/alice/inbox"),
            Some("social.example")
        );
        assert_eq!(https_host("http://mastodon.social/inbox"), None);
        assert_eq!(https_host("not a url"), None);
    }

    #[test]
    fn normalizes_federation_hosts_for_owner_commands() {
        assert_eq!(
            normalize_host("https://Mastodon.Social/inbox"),
            "mastodon.social"
        );
        assert_eq!(
            normalize_host("http://example.com/users/alice"),
            "example.com"
        );
        assert_eq!(normalize_host(" social.example/ "), "social.example");
    }
}
