// Vercel HTTP provider
//
// This provider uses reqwest for HTTP requests from Vercel Edge Functions.

use async_trait::async_trait;
use dais_core::traits::HttpProvider;
use dais_core::types::{CoreResult, CoreError, HttpRequest, HttpResponse};
use reqwest::Client;
use std::collections::HashMap;

/// Vercel HTTP provider
///
/// Uses reqwest client for HTTP requests with proper header handling.
pub struct VercelHttpProvider {
    client: Client,
}

impl VercelHttpProvider {
    /// Create a new Vercel HTTP provider
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("dais/1.2.0 (Vercel Edge Functions)")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
}

impl Default for VercelHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpProvider for VercelHttpProvider {
    async fn fetch(&self, request: HttpRequest) -> CoreResult<HttpResponse> {
        // Build request
        let mut req = match request.method.as_str() {
            "GET" => self.client.get(&request.url),
            "POST" => self.client.post(&request.url),
            "PUT" => self.client.put(&request.url),
            "DELETE" => self.client.delete(&request.url),
            "PATCH" => self.client.patch(&request.url),
            "HEAD" => self.client.head(&request.url),
            method => {
                return Err(CoreError::HttpError(format!("Unsupported HTTP method: {}", method)))
            }
        };

        // Add headers
        for (key, value) in &request.headers {
            req = req.header(key, value);
        }

        // Add body if present
        if let Some(body) = &request.body {
            req = req.body(body.clone());
        }

        // Send request
        let response = req
            .send()
            .await
            .map_err(|e| CoreError::HttpError(format!("HTTP request failed: {}", e)))?;

        // Extract status
        let status = response.status().as_u16();

        // Extract headers
        let mut headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(key.as_str().to_string(), value_str.to_string());
            }
        }

        // Extract body
        let body = response
            .bytes()
            .await
            .map_err(|e| CoreError::HttpError(format!("Failed to read response body: {}", e)))?
            .to_vec();

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_get() {
        let provider = VercelHttpProvider::new();

        let request = HttpRequest {
            method: "GET".to_string(),
            url: "https://httpbin.org/get".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        let response = provider.fetch(request).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(!response.body.is_empty());
    }

    #[tokio::test]
    async fn test_http_post() {
        let provider = VercelHttpProvider::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let request = HttpRequest {
            method: "POST".to_string(),
            url: "https://httpbin.org/post".to_string(),
            headers,
            body: Some(b"{\"test\":\"data\"}".to_vec()),
        };

        let response = provider.fetch(request).await.unwrap();
        assert_eq!(response.status, 200);
    }

    #[tokio::test]
    async fn test_http_custom_headers() {
        let provider = VercelHttpProvider::new();

        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());

        let request = HttpRequest {
            method: "GET".to_string(),
            url: "https://httpbin.org/headers".to_string(),
            headers,
            body: None,
        };

        let response = provider.fetch(request).await.unwrap();
        assert_eq!(response.status, 200);

        let body_str = String::from_utf8_lossy(&response.body);
        assert!(body_str.contains("X-Custom-Header"));
    }
}
