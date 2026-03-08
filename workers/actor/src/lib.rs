use worker::*;
use shared::activitypub::Person;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/users/:username", handle_actor)
        .run(req, env)
        .await
}

async fn handle_actor(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        let mut headers = Headers::new();
        headers.set("Access-Control-Allow-Origin", "*")?;
        headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
        headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;
        return Ok(Response::empty()?.with_headers(headers));
    }

    // Check Accept header for content negotiation
    let accept_header = req.headers().get("Accept")?.unwrap_or_default();
    let wants_html = accept_header.contains("text/html") && !accept_header.contains("application/activity+json");

    // CORS headers
    let mut headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;

    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    // Get D1 database
    let db = ctx.env.d1("DB").expect("D1 database binding not found");

    // Query for actor
    let query = "SELECT id, username, display_name, summary, public_key FROM actors WHERE username = ?";
    let statement = db.prepare(query).bind(&[username.into()])?;

    let result = statement.first::<serde_json::Value>(None).await?;

    let actor_data = match result {
        Some(data) => data,
        None => {
            console_log!("Actor not found: {}", username);
            return Response::error("Actor not found", 404);
        }
    };

    console_log!("Found actor: {:?}", actor_data);

    // Extract fields
    let actor_username = actor_data["username"]
        .as_str()
        .ok_or("Missing username")?;

    let public_key_pem = actor_data["public_key"]
        .as_str()
        .ok_or("Missing public key")?;

    // Build Person object
    let mut person = Person::new(
        format!("https://social.dais.social/users/{}", actor_username),
        actor_username.to_string(),
        "social.dais.social".to_string(),
        public_key_pem.to_string(),
    );

    // Add optional fields if present
    if let Some(name) = actor_data["display_name"].as_str() {
        if !name.is_empty() {
            person = person.with_name(name.to_string());
        }
    }

    if let Some(summary) = actor_data["summary"].as_str() {
        if !summary.is_empty() {
            person = person.with_summary(summary.to_string());
        }
    }

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_profile_html(&person, actor_username);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&person)?.with_headers(headers))
    }
}

fn render_profile_html(person: &Person, username: &str) -> String {
    let display_name = person.name.as_ref().unwrap_or(&person.preferred_username);
    let summary = person.summary.as_deref().unwrap_or("");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{display_name} (@{username}@dais.social)</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #1a1a1a;
            color: #e0e0e0;
            line-height: 1.6;
            padding: 20px;
        }}
        .container {{
            max-width: 600px;
            margin: 40px auto;
            background: #2a2a2a;
            border-radius: 12px;
            padding: 40px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
        }}
        .avatar {{
            width: 120px;
            height: 120px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin: 0 auto 20px;
            font-size: 48px;
            color: white;
        }}
        h1 {{
            font-size: 28px;
            margin-bottom: 8px;
            color: #ffffff;
        }}
        .handle {{
            color: #8899a6;
            font-size: 16px;
        }}
        .bio {{
            margin: 30px 0;
            padding: 20px;
            background: #1a1a1a;
            border-radius: 8px;
            color: #d0d0d0;
        }}
        .stats {{
            display: flex;
            justify-content: space-around;
            margin: 30px 0;
            padding: 20px 0;
            border-top: 1px solid #3a3a3a;
            border-bottom: 1px solid #3a3a3a;
        }}
        .stat {{
            text-align: center;
        }}
        .stat-value {{
            font-size: 24px;
            font-weight: bold;
            color: #667eea;
        }}
        .stat-label {{
            color: #8899a6;
            font-size: 14px;
            margin-top: 4px;
        }}
        .footer {{
            text-align: center;
            margin-top: 40px;
            padding-top: 20px;
            border-top: 1px solid #3a3a3a;
            color: #8899a6;
            font-size: 14px;
        }}
        .footer a {{
            color: #667eea;
            text-decoration: none;
        }}
        .footer a:hover {{
            text-decoration: underline;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div class="avatar">{}</div>
            <h1>{}</h1>
            <div class="handle">@{}@dais.social</div>
        </div>

        {}

        <div class="stats">
            <div class="stat">
                <div class="stat-value">0</div>
                <div class="stat-label">Posts</div>
            </div>
            <div class="stat">
                <div class="stat-value">0</div>
                <div class="stat-label">Following</div>
            </div>
            <div class="stat">
                <div class="stat-value">0</div>
                <div class="stat-label">Followers</div>
            </div>
        </div>

        <div class="footer">
            <p>Powered by <a href="https://dais.social">dais</a> - Self-hosted ActivityPub on Cloudflare Workers</p>
            <p style="margin-top: 10px;">
                <a href="{}">View ActivityPub Profile (JSON)</a>
            </p>
        </div>
    </div>
</body>
</html>"#,
        display_name.chars().next().unwrap_or('?').to_uppercase(),
        display_name,
        username,
        if !summary.is_empty() {
            format!(r#"<div class="bio">{}</div>"#, summary)
        } else {
            String::new()
        },
        person.id
    )
}
