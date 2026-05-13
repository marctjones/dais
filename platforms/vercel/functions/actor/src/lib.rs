// Actor function for Vercel Edge Functions
//
// Handles actor profile requests and collections (followers, following)

use dais_core::{DaisCore, CoreConfig, CoreError};
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelHttpProvider, VercelQueueProvider};
use serde_json::json;
use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    // Parse URL path to get username
    let path = req.uri().path();
    let username = path
        .strip_prefix("/users/")
        .and_then(|p| p.split('/').next())
        .ok_or("Invalid path")?
        .to_string();

    // Initialize providers
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable required")?;

    let db = NeonProvider::new(&database_url)
        .await
        .map_err(|e| format!("Database connection failed: {}", e))?;

    let blob_token = std::env::var("BLOB_READ_WRITE_TOKEN").unwrap_or_default();
    let storage = VercelBlobProvider::new(&blob_token);
    let queue = VercelQueueProvider::from_env();
    let http = VercelHttpProvider::new();

    // Create config from environment
    let config = CoreConfig {
        activitypub_domain: std::env::var("DOMAIN").unwrap_or_else(|_| "localhost".to_string()),
        pds_domain: std::env::var("PDS_DOMAIN").unwrap_or_else(|_| "localhost".to_string()),
        username: std::env::var("USERNAME").unwrap_or_else(|_| "social".to_string()),
        private_key: std::env::var("PRIVATE_KEY").unwrap_or_default(),
        public_key: std::env::var("PUBLIC_KEY").unwrap_or_default(),
    };

    // Create core instance
    let core = DaisCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
        Box::new(http),
        config,
    );

    // Check if this is a collection request
    if path.ends_with("/followers") {
        // Get followers collection
        match core.get_followers(username, None).await {
            Ok(collection) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/activity+json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&collection)?.into())?),
            Err(e) => error_response(e),
        }
    } else if path.ends_with("/following") {
        // Get following collection
        match core.get_following(username, None).await {
            Ok(collection) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/activity+json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&collection)?.into())?),
            Err(e) => error_response(e),
        }
    } else {
        // Get actor profile
        match core.get_actor(username).await {
            Ok(actor) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/activity+json")
                .header("Access-Control-Allow-Origin", "*")
                .body(serde_json::to_string(&actor)?.into())?),
            Err(e) => error_response(e),
        }
    }
}

fn error_response(e: CoreError) -> Result<Response<Body>, Error> {
    let (status, message) = match e {
        CoreError::NotFound(_) => (StatusCode::NOT_FOUND, "User not found"),
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
