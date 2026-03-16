/// Refactored Landing worker
///
/// This worker serves the landing page for the dais instance.
/// It provides information about the instance and links to the actor profile.

use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();

    if path == "/" {
        handle_landing(env).await
    } else if path == "/health" {
        Response::ok("OK")
    } else {
        Response::error("Not Found", 404)
    }
}

async fn handle_landing(env: Env) -> Result<Response> {
    // Get configuration from environment
    let domain = env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", domain));

    let username = env.var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    let actor_url = format!("https://{}/users/{}", activitypub_domain, username);

    // Simple HTML landing page
    let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>dais - Single-User ActivityPub Server</title>
    <style>
        body {{
            font-family: system-ui, -apple-system, sans-serif;
            max-width: 800px;
            margin: 0 auto;
            padding: 2rem;
            line-height: 1.6;
            color: #333;
        }}
        h1 {{
            color: #2563eb;
        }}
        a {{
            color: #2563eb;
            text-decoration: none;
        }}
        a:hover {{
            text-decoration: underline;
        }}
        .card {{
            background: #f3f4f6;
            padding: 1.5rem;
            border-radius: 0.5rem;
            margin: 1.5rem 0;
        }}
        .footer {{
            margin-top: 3rem;
            padding-top: 1rem;
            border-top: 1px solid #e5e7eb;
            color: #6b7280;
            font-size: 0.875rem;
        }}
    </style>
</head>
<body>
    <h1>🌐 dais</h1>
    <p>A single-user ActivityPub and AT Protocol server for complete ownership of your social media presence.</p>

    <div class="card">
        <h2>This Instance</h2>
        <p><strong>Domain:</strong> {domain}</p>
        <p><strong>Username:</strong> @{username}@{activitypub_domain}</p>
        <p><strong>ActivityPub Actor:</strong> <a href="{actor_url}">{actor_url}</a></p>
    </div>

    <h2>Features</h2>
    <ul>
        <li>✅ Full ActivityPub implementation (Mastodon, Pleroma, etc.)</li>
        <li>✅ AT Protocol support (Bluesky)</li>
        <li>✅ HTTP signature verification</li>
        <li>✅ Content moderation (optional)</li>
        <li>✅ Platform-agnostic architecture</li>
    </ul>

    <h2>Links</h2>
    <ul>
        <li><a href="{actor_url}">View Profile</a></li>
        <li><a href="https://github.com/marctjones/dais">GitHub Repository</a></li>
        <li><a href="https://dais.social">Documentation</a></li>
    </ul>

    <div class="footer">
        <p>Powered by <a href="https://github.com/marctjones/dais">dais v1.1</a> | Running on Cloudflare Workers</p>
    </div>
</body>
</html>"#, domain = domain, activitypub_domain = activitypub_domain, username = username, actor_url = actor_url);

    Response::from_html(html)
}
