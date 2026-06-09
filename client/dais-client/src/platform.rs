//! Native implementations of core's platform traits, so the client reuses the
//! *same* audited federation code the Cloudflare Workers run.
//!
//! - [`ReqwestHttp`] implements `dais_core::traits::HttpProvider` over `reqwest`.
//! - [`D1Db`] implements `dais_core::traits::DatabaseProvider` over the Cloudflare
//!   D1 HTTP API (parameterized — no string-interpolated SQL).

use std::collections::HashMap;

use async_trait::async_trait;
use dais_core::traits::{
    DatabaseDialect, DatabaseProvider, HttpProvider, Method, PlatformError, PlatformResult,
    Request as CoreRequest, Response as CoreResponse, Row, Statement,
};
use serde_json::Value;

use crate::config::D1Config;
use crate::d1::D1Client;
use crate::error::Result;

/// `HttpProvider` backed by reqwest. Keeps two clients so the per-request
/// `follow_redirects` flag is honored (reqwest's redirect policy is client-level).
pub struct ReqwestHttp {
    redirect: reqwest::Client,
    no_redirect: reqwest::Client,
}

impl ReqwestHttp {
    pub fn new() -> Self {
        let redirect = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("dais/0.1")
            .build()
            .unwrap_or_default();
        let no_redirect = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("dais/0.1")
            .build()
            .unwrap_or_default();
        ReqwestHttp {
            redirect,
            no_redirect,
        }
    }
}

impl Default for ReqwestHttp {
    fn default() -> Self {
        Self::new()
    }
}

fn to_reqwest_method(m: Method) -> reqwest::Method {
    match m {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Delete => reqwest::Method::DELETE,
        Method::Patch => reqwest::Method::PATCH,
        Method::Head => reqwest::Method::HEAD,
        Method::Options => reqwest::Method::OPTIONS,
    }
}

#[async_trait(?Send)]
impl HttpProvider for ReqwestHttp {
    async fn fetch(&self, request: CoreRequest) -> PlatformResult<CoreResponse> {
        let client = if request.follow_redirects {
            &self.redirect
        } else {
            &self.no_redirect
        };

        let mut rb = client.request(to_reqwest_method(request.method), &request.url);
        for (k, v) in &request.headers {
            rb = rb.header(k, v);
        }
        if let Some(secs) = request.timeout {
            rb = rb.timeout(std::time::Duration::from_secs(secs as u64));
        }
        if let Some(body) = request.body {
            rb = rb.body(body);
        }

        let resp = rb
            .send()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let url = resp.url().to_string();
        let mut headers = HashMap::new();
        for (k, v) in resp.headers() {
            if let Ok(s) = v.to_str() {
                headers.insert(k.as_str().to_string(), s.to_string());
            }
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?
            .to_vec();

        Ok(CoreResponse {
            status,
            headers,
            body,
            url,
        })
    }
}

/// `DatabaseProvider` backed by the Cloudflare D1 HTTP API.
pub struct D1Db {
    client: D1Client,
}

impl D1Db {
    pub fn from_config(cfg: &D1Config) -> Result<Self> {
        Ok(D1Db {
            client: D1Client::from_config(cfg)?,
        })
    }

    pub fn new(client: D1Client) -> Self {
        D1Db { client }
    }
}

#[async_trait(?Send)]
impl DatabaseProvider for D1Db {
    async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>> {
        let result = self
            .client
            .query(sql, params)
            .await
            .map_err(|e| PlatformError::Database(e.to_string()))?;

        let rows = result
            .rows
            .into_iter()
            .map(|v| {
                let mut row = Row::new();
                if let Value::Object(map) = v {
                    for (k, val) in map {
                        row.insert(k, val);
                    }
                }
                row
            })
            .collect();
        Ok(rows)
    }

    async fn batch(&self, statements: Vec<Statement>) -> PlatformResult<()> {
        // D1's query endpoint runs one parameterized statement at a time; run the
        // batch sequentially. (Our writes are individually small.)
        for st in statements {
            self.client
                .query(&st.sql, &st.params)
                .await
                .map_err(|e| PlatformError::Database(e.to_string()))?;
        }
        Ok(())
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::SQLite
    }
}
