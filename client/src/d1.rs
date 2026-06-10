use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
pub struct D1Post {
    pub id: String,
    pub content: String,
    pub visibility: Option<String>,
    pub protocol: Option<String>,
    pub published_at: Option<String>,
    pub atproto_uri: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct D1User {
    pub actor_id: String,
    pub relation: String,
    pub status: String,
    pub created_at: Option<String>,
}

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

#[derive(Debug)]
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
                   published_at, atproto_uri
            FROM posts
            ORDER BY published_at DESC
            LIMIT {limit}
            "#
        );
        self.query(&sql)
    }

    pub async fn search_posts(&self, needle: &str, limit: u16) -> Result<Vec<D1Post>> {
        let limit = clamp_limit(limit);
        let needle = sql_like_escape(needle);
        let sql = format!(
            r#"
            SELECT id, content, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, atproto_uri
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
                   content, visibility, published_at, updated_at, protocol
            FROM timeline_posts
            WHERE deleted_at IS NULL {before_filter}
            ORDER BY published_at DESC
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
