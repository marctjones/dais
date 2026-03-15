/// HTTP client abstraction trait for platform-agnostic requests
///
/// Implementations:
/// - Cloudflare: Cloudflare Workers fetch API
/// - Vercel: Node.js fetch (global)
/// - Netlify: Netlify Functions fetch
/// - Generic: reqwest, hyper, etc.

use super::PlatformResult;
use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait(?Send)]
pub trait HttpProvider {
    /// Perform an HTTP request
    ///
    /// # Example
    /// ```rust,ignore
    /// let response = http.fetch(Request {
    ///     url: "https://mastodon.social/inbox".into(),
    ///     method: Method::Post,
    ///     headers: headers!{
    ///         "Content-Type" => "application/activity+json",
    ///         "Signature" => signature,
    ///     },
    ///     body: Some(activity_json.into_bytes()),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    async fn fetch(&self, request: Request) -> PlatformResult<Response>;

    /// Convenience: GET request
    async fn get(&self, url: &str) -> PlatformResult<Response> {
        self.fetch(Request {
            url: url.to_string(),
            method: Method::Get,
            ..Default::default()
        })
        .await
    }

    /// Convenience: POST request with JSON body
    async fn post_json(&self, url: &str, body: &str) -> PlatformResult<Response> {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        self.fetch(Request {
            url: url.to_string(),
            method: Method::Post,
            headers,
            body: Some(body.as_bytes().to_vec()),
            ..Default::default()
        })
        .await
    }
}

/// HTTP request
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// Request URL
    pub url: String,

    /// HTTP method
    pub method: Method,

    /// Request headers
    pub headers: HashMap<String, String>,

    /// Request body
    pub body: Option<Vec<u8>>,

    /// Request timeout in seconds
    pub timeout: Option<u32>,

    /// Follow redirects
    pub follow_redirects: bool,
}

impl Request {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    pub fn json_body(mut self, json: &str) -> Self {
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(json.as_bytes().to_vec());
        self
    }

    pub fn timeout(mut self, seconds: u32) -> Self {
        self.timeout = Some(seconds);
        self
    }

    pub fn follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = follow;
        self
    }
}

/// HTTP response
#[derive(Debug, Clone)]
pub struct Response {
    /// HTTP status code
    pub status: u16,

    /// Response headers
    pub headers: HashMap<String, String>,

    /// Response body
    pub body: Vec<u8>,

    /// Final URL (after redirects)
    pub url: String,
}

impl Response {
    /// Check if response is successful (2xx status)
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// Get response body as UTF-8 string
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    /// Parse response body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// Get header value (case-insensitive)
    pub fn header(&self, key: &str) -> Option<&String> {
        let key_lower = key.to_lowercase();
        self.headers.iter()
            .find(|(k, _)| k.to_lowercase() == key_lower)
            .map(|(_, v)| v)
    }

    /// Get Content-Type header
    pub fn content_type(&self) -> Option<&String> {
        self.header("Content-Type")
    }
}

/// HTTP methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl Default for Method {
    fn default() -> Self {
        Method::Get
    }
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }
}

impl From<&str> for Method {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            "PATCH" => Method::Patch,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            _ => Method::Get,
        }
    }
}

/// Helper macro for creating headers
#[macro_export]
macro_rules! headers {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut map = std::collections::HashMap::new();
        $(
            map.insert($key.to_string(), $value.to_string());
        )*
        map
    }};
}
