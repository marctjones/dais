use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use dais_core::{DaisCore, CoreConfig};
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelQueueProvider, VercelHttpProvider};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    // Initialize providers from environment variables
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL not set")?;
    let blob_token = std::env::var("BLOB_READ_WRITE_TOKEN")
        .map_err(|_| "BLOB_READ_WRITE_TOKEN not set")?;

    let db = NeonProvider::new(&database_url).await
        .map_err(|e| format!("Database error: {}", e))?;
    let storage = VercelBlobProvider::new(&blob_token);
    let queue = VercelQueueProvider::from_env();
    let http = VercelHttpProvider::new();

    let config = CoreConfig {
        activitypub_domain: std::env::var("DOMAIN").unwrap_or_else(|_| "localhost".to_string()),
        pds_domain: std::env::var("PDS_DOMAIN").unwrap_or_else(|_| "localhost".to_string()),
        username: std::env::var("USERNAME").unwrap_or_else(|_| "social".to_string()),
        private_key: std::env::var("PRIVATE_KEY").unwrap_or_default(),
        public_key: std::env::var("PUBLIC_KEY").unwrap_or_default(),
    };

    let core = DaisCore::new(
        Box::new(db),
        Box::new(storage),
        Box::new(queue),
        Box::new(http),
        config,
    );

    // Extract username from path /users/{username}/inbox
    let path = req.uri().path();
    let username = path.strip_prefix("/users/")
        .and_then(|p| p.split('/').next())
        .ok_or("Invalid inbox path")?;

    // Parse incoming activity
    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
    let activity_json = String::from_utf8(body_bytes.to_vec())
        .map_err(|e| format!("Invalid UTF-8: {}", e))?;

    // Build actor URL
    let domain = std::env::var("DOMAIN").unwrap_or_else(|_| "localhost".to_string());
    let actor_url = format!("https://{}/users/{}", domain, username);

    // Process the activity
    match core.handle_inbox(activity_json, actor_url, None).await {
        Ok(_) => Ok(Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(Body::Empty)?),
        Err(e) => {
            eprintln!("Inbox error: {}", e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(format!("Error: {}", e).into())?)
        }
    }
}
