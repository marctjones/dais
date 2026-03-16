// WebFinger function for Vercel Edge Functions
//
// This function handles .well-known/webfinger requests using the dais-core library
// and Vercel-specific platform bindings.

use dais_core::DaisCore;
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelHttpProvider, VercelQueueProvider};
use serde_json::json;
use std::collections::HashMap;
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    // Parse query parameters
    let uri = req.uri();
    let query_params: HashMap<String, String> = uri
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Get resource parameter
    let resource = match query_params.get("resource") {
        Some(r) => r,
        None => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .body(
                    json!({
                        "error": "Missing resource parameter"
                    })
                    .to_string()
                    .into(),
                )?);
        }
    };

    // Initialize providers
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL environment variable must be set");

    let db = NeonProvider::new(&database_url)
        .await
        .map_err(|e| format!("Database connection failed: {}", e))?;

    let blob_token = std::env::var("BLOB_READ_WRITE_TOKEN")
        .unwrap_or_else(|_| String::new());
    let storage = VercelBlobProvider::new(&blob_token);

    let queue = VercelQueueProvider::from_env();
    let http = VercelHttpProvider::new();

    // Create core instance
    let core = DaisCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
        Box::new(http),
    );

    // Process WebFinger request
    match core.webfinger(resource).await {
        Ok(response) => {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/jrd+json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&response)?.into())?)
        }
        Err(e) => {
            let (status, message) = match e {
                dais_core::types::CoreError::NotFound => {
                    (StatusCode::NOT_FOUND, "Resource not found")
                }
                dais_core::types::CoreError::InvalidResource => {
                    (StatusCode::BAD_REQUEST, "Invalid resource format")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
            };

            Ok(Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(
                    json!({
                        "error": message,
                        "details": e.to_string()
                    })
                    .to_string()
                    .into(),
                )?)
        }
    }
}
