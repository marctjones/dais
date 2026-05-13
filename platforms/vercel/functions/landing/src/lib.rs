use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use dais_core::{DaisCore, CoreConfig};
use dais_vercel::{NeonProvider, VercelBlobProvider, VercelQueueProvider, VercelHttpProvider};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(_req: Request) -> Result<Response<Body>, Error> {
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

    // Get theme from environment
    let theme = std::env::var("THEME").unwrap_or_else(|_| "cat-light".to_string());
    let domain = std::env::var("DOMAIN").unwrap_or_else(|_| "localhost".to_string());

    // Generate simple landing page
    let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>dais on Vercel</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 2rem;
            line-height: 1.6;
        }}
        h1 {{ color: #333; }}
        .info {{ background: #f5f5f5; padding: 1rem; border-radius: 8px; }}
        .stat {{ background: white; padding: 1rem; border-radius: 4px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); margin-top: 1rem; }}
    </style>
</head>
<body>
    <h1>dais on Vercel</h1>
    <div class="info">
        <p><strong>Instance:</strong> {domain}</p>
        <p><strong>Theme:</strong> {theme}</p>
        <p><strong>Platform:</strong> Vercel Edge Functions</p>
        <p><strong>Version:</strong> 1.2.0</p>
    </div>
    <div class="stat">
        <p>This is a dais ActivityPub server running on Vercel Edge Functions.</p>
        <p>Platform bindings: Neon PostgreSQL, Vercel Blob, Upstash Redis</p>
    </div>
    <p style="margin-top: 2rem; color: #666; font-size: 0.9rem;">
        Powered by <a href="https://github.com/marctjones/dais">dais</a> -
        A multi-platform ActivityPub server
    </p>
</body>
</html>"#,
        domain = domain,
        theme = theme,
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(html.into())?)
}
