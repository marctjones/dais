use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use dais_core::{DaisCore, CoreConfig};
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelQueueProvider, VercelHttpProvider};

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

    let path = req.uri().path();
    let method = req.method();

    // AT Protocol endpoints
    match (method.as_str(), path) {
        ("GET", "/xrpc/com.atproto.server.describeServer") => {
            // AT Protocol server description
            let response = serde_json::json!({
                "did": "did:web:localhost",
                "availableUserDomains": [],
                "inviteCodeRequired": false
            });
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(response.to_string().into())?)
        }
        ("POST", "/xrpc/com.atproto.repo.createRecord") => {
            // Create record - not yet implemented
            Ok(Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body("AT Protocol create record not yet implemented".into())?)
        }
        ("GET", path) if path.starts_with("/xrpc/com.atproto.repo.getRecord") => {
            // Get record - not yet implemented
            Ok(Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .body("AT Protocol get record not yet implemented".into())?)
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("AT Protocol endpoint not found".into())?),
    }
}
