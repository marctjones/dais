//! Cloudflare D1 HTTP API client.
//!
//! Replaces shelling out to `wrangler d1 execute` (CLIENT_REDESIGN.md §3.1): direct
//! HTTPS to the D1 query endpoint, structured errors, no subprocess. Used to refresh
//! the local store from prod and to run light management queries.

use serde::Deserialize;
use serde_json::Value;

use crate::config::D1Config;
use crate::error::{Error, Result};

/// A thin async client over the D1 `query` endpoint.
pub struct D1Client {
    account_id: String,
    database_id: String,
    api_token: String,
    http: reqwest::Client,
}

/// The rows returned by a successful query (each row is a JSON object).
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct D1Envelope {
    success: bool,
    #[serde(default)]
    errors: Vec<D1ApiError>,
    #[serde(default)]
    result: Vec<D1QueryBlock>,
}

#[derive(Debug, Deserialize)]
struct D1ApiError {
    #[serde(default)]
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct D1QueryBlock {
    #[serde(default)]
    results: Vec<Value>,
}

impl D1Client {
    /// Build a client from config, erroring if credentials are incomplete.
    pub fn from_config(cfg: &D1Config) -> Result<Self> {
        let account_id = cfg
            .account_id
            .clone()
            .ok_or_else(|| Error::NotConfigured("d1.account_id".into()))?;
        let database_id = cfg
            .database_id
            .clone()
            .ok_or_else(|| Error::NotConfigured("d1.database_id".into()))?;
        let api_token = cfg
            .api_token
            .clone()
            .ok_or_else(|| Error::NotConfigured("d1.api_token".into()))?;
        Ok(D1Client {
            account_id,
            database_id,
            api_token,
            http: reqwest::Client::new(),
        })
    }

    fn endpoint(&self) -> String {
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/d1/database/{}/query",
            self.account_id, self.database_id
        )
    }

    /// Run a parameterized SQL statement; returns the result rows.
    pub async fn query(&self, sql: &str, bind: &[Value]) -> Result<QueryResult> {
        let body = serde_json::json!({ "sql": sql, "params": bind });
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let env: D1Envelope = resp.json().await.map_err(|e| {
            Error::D1(format!("decoding D1 response (HTTP {status}): {e}"))
        })?;

        if !env.success {
            let msg = env
                .errors
                .iter()
                .map(|e| format!("[{}] {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::D1(if msg.is_empty() {
                format!("D1 query failed (HTTP {status})")
            } else {
                msg
            }));
        }

        let rows = env
            .result
            .into_iter()
            .flat_map(|b| b.results)
            .collect::<Vec<_>>();
        Ok(QueryResult { rows })
    }

    /// Convenience: a `SELECT 1` round-trip to confirm credentials/connectivity.
    pub async fn ping(&self) -> Result<()> {
        self.query("SELECT 1 AS ok", &[]).await.map(|_| ())
    }
}
