/// HTTP provider implementation for Cloudflare Workers
///
/// Implements the HttpProvider trait using the Workers fetch API

use dais_core::traits::{HttpProvider, PlatformError, PlatformResult, Request, Response, Method};
use async_trait::async_trait;
use worker::Fetch;

pub struct WorkerHttpProvider;

impl WorkerHttpProvider {
    /// Create a new WorkerHttpProvider
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkerHttpProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl HttpProvider for WorkerHttpProvider {
    async fn fetch(&self, request: Request) -> PlatformResult<Response> {
        // Build worker::Request from our Request
        let method = match request.method {
            Method::Get => worker::Method::Get,
            Method::Post => worker::Method::Post,
            Method::Put => worker::Method::Put,
            Method::Delete => worker::Method::Delete,
            Method::Patch => worker::Method::Patch,
            Method::Head => worker::Method::Head,
            Method::Options => worker::Method::Options,
        };

        let mut init = worker::RequestInit::new();
        init.with_method(method);

        // Add headers
        let mut headers = worker::Headers::new();
        for (key, value) in &request.headers {
            headers
                .set(key, value)
                .map_err(|e| PlatformError::Http(format!("Failed to set header: {:?}", e)))?;
        }
        init.with_headers(headers);

        // Add body if present
        if let Some(body) = request.body {
            init.with_body(Some(wasm_bindgen::JsValue::from(
                js_sys::Uint8Array::from(&body[..]),
            )));
        }

        // Create worker::Request
        let worker_request = worker::Request::new_with_init(&request.url, &init)
            .map_err(|e| PlatformError::Http(format!("Failed to create request: {:?}", e)))?;

        // Perform fetch
        let mut worker_response = Fetch::Request(worker_request)
            .send()
            .await
            .map_err(|e| PlatformError::Http(format!("Fetch failed: {:?}", e)))?;

        // Extract response data
        let status = worker_response.status_code();

        // Extract headers
        let mut response_headers = std::collections::HashMap::new();
        let headers = worker_response.headers();
        for (key, value) in headers.entries() {
            response_headers.insert(key, value);
        }

        // Read body
        let body = worker_response
            .bytes()
            .await
            .map_err(|e| PlatformError::Http(format!("Failed to read response body: {:?}", e)))?;

        Ok(Response {
            status,
            headers: response_headers,
            body,
            url: request.url.clone(),  // Use original request URL
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_provider() {
        // Can't test with real HTTP in unit tests
        // In integration tests with wrangler dev, we can test real HTTP requests
    }
}
