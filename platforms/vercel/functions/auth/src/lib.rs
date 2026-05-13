use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use dais_core::{DaisCore, CoreConfig};
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelQueueProvider, VercelHttpProvider};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    success: bool,
    token: Option<String>,
    message: Option<String>,
}

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

    match (method.as_str(), path) {
        ("POST", "/auth/login") => {
            // Authentication not yet implemented
            let response = AuthResponse {
                success: false,
                token: None,
                message: Some("Authentication not yet implemented".to_string()),
            };
            Ok(Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&response)?.into())?)
        }
        ("POST", "/auth/verify") => {
            // Token verification not yet implemented
            let response = AuthResponse {
                success: false,
                token: None,
                message: Some("Token verification not yet implemented".to_string()),
            };
            Ok(Response::builder()
                .status(StatusCode::NOT_IMPLEMENTED)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&response)?.into())?)
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("Not found".into())?),
    }
}
