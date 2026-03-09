use worker::*;
use shared::activitypub::Person;
use shared::theme::Theme;

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

    // Get actor ID as string for queries
    let actor_id_str = actor_data["id"].as_str().ok_or("Missing actor id")?;

    // Query for post count
    let post_count_query = "SELECT COUNT(*) as count FROM posts WHERE actor_id = ?";
    let post_count_stmt = db.prepare(post_count_query).bind(&[actor_id_str.into()])?;
    let post_count_result = post_count_stmt.first::<serde_json::Value>(None).await?;
    let post_count = post_count_result
        .and_then(|v| v["count"].as_u64())
        .unwrap_or(0) as usize;

    // Query for follower count (approved only)
    let follower_count_query = "SELECT COUNT(*) as count FROM followers WHERE actor_id = ? AND status = 'approved'";
    let follower_count_stmt = db.prepare(follower_count_query).bind(&[actor_id_str.into()])?;
    let follower_count_result = follower_count_stmt.first::<serde_json::Value>(None).await?;
    let follower_count = follower_count_result
        .and_then(|v| v["count"].as_u64())
        .unwrap_or(0) as usize;

    // Get theme from environment (default to "dais")
    let theme_name = ctx.env.var("THEME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais".to_string());
    let theme = Theme::from_name(&theme_name);

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_profile_html(&person, actor_username, post_count, follower_count, &theme);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&person)?.with_headers(headers))
    }
}

fn render_profile_html(person: &Person, username: &str, post_count: usize, follower_count: usize, theme: &Theme) -> String {
    let display_name = person.name.as_ref().unwrap_or(&person.preferred_username);
    let summary = person.summary.as_deref().unwrap_or("");

    let light = &theme.light;
    let dark = &theme.dark;

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{display_name} (@{username}@dais.social)</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Helvetica Neue', Arial, sans-serif;
            background: {bg_primary};
            color: {text_primary};
            line-height: 1.6;
            padding: 20px;
        }}
        .container {{
            max-width: 600px;
            margin: 40px auto;
        }}
        .profile-card {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 48px;
            margin-bottom: 20px;
        }}
        .header {{
            text-align: center;
            margin-bottom: 32px;
        }}
        .avatar {{
            width: 120px;
            height: 120px;
            background: linear-gradient(135deg, {accent_primary} 0%, {accent_hover} 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin: 0 auto 20px;
            font-size: 48px;
            color: white;
            font-weight: 700;
        }}
        h1 {{
            font-size: 32px;
            margin-bottom: 8px;
            color: {text_primary};
            font-weight: 700;
        }}
        .handle {{
            color: {text_secondary};
            font-size: 16px;
        }}
        .bio {{
            margin: 32px 0;
            padding: 24px;
            background: {bg_primary};
            border-radius: 12px;
            color: {text_primary};
            border-left: 4px solid {accent_primary};
        }}
        .stats {{
            display: flex;
            justify-content: space-around;
            margin: 32px 0;
            padding: 24px 0;
            border-top: 1px solid {border};
            border-bottom: 1px solid {border};
        }}
        .stat {{
            text-align: center;
        }}
        .stat-value {{
            font-size: 28px;
            font-weight: 700;
            color: {accent_primary};
        }}
        .stat-label {{
            color: {text_secondary};
            font-size: 14px;
            margin-top: 4px;
        }}
        .actions {{
            display: flex;
            gap: 12px;
            justify-content: center;
            margin-top: 32px;
        }}
        .button {{
            padding: 12px 24px;
            border-radius: 8px;
            font-size: 15px;
            font-weight: 600;
            text-decoration: none;
            transition: all 0.2s ease;
        }}
        .button-primary {{
            background: {accent_primary};
            color: white;
        }}
        .button-primary:hover {{
            background: {accent_hover};
        }}
        .button-secondary {{
            background: {bg_primary};
            color: {text_primary};
            border: 2px solid {border};
        }}
        .button-secondary:hover {{
            border-color: {accent_primary};
            color: {accent_hover};
        }}
        .footer {{
            text-align: center;
            margin-top: 32px;
            padding-top: 24px;
            border-top: 1px solid {border};
            color: {text_secondary};
            font-size: 14px;
        }}
        .footer a {{
            color: {accent_hover};
            text-decoration: none;
            font-weight: 500;
        }}
        .footer a:hover {{
            text-decoration: underline;
        }}
        @media (prefers-color-scheme: dark) {{
            body {{
                background: {dark_bg_primary};
                color: {dark_text_primary};
            }}
            .profile-card {{
                background: {dark_bg_secondary};
            }}
            h1 {{
                color: {dark_text_primary};
            }}
            .handle, .stat-label {{
                color: {dark_text_secondary};
            }}
            .bio {{
                background: {dark_bg_primary};
                color: {dark_text_primary};
                border-left-color: {dark_accent_primary};
            }}
            .stat-value {{
                color: {dark_accent_primary};
            }}
            .stats {{
                border-top-color: {dark_border};
                border-bottom-color: {dark_border};
            }}
            .button-primary {{
                background: {dark_accent_primary};
                color: {dark_bg_primary};
            }}
            .button-primary:hover {{
                background: {dark_accent_hover};
            }}
            .button-secondary {{
                background: {dark_bg_primary};
                color: {dark_text_primary};
                border-color: {dark_border};
            }}
            .button-secondary:hover {{
                border-color: {dark_accent_primary};
                color: {dark_accent_primary};
            }}
            .footer {{
                border-top-color: {dark_border};
                color: {dark_text_secondary};
            }}
            .footer a {{
                color: {dark_accent_hover};
            }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="profile-card">
            <div class="header">
                <div class="avatar">{avatar_initial}</div>
                <h1>{display_name}</h1>
                <div class="handle">@{handle_username}@dais.social</div>
            </div>

            {bio_html}

            <div class="stats">
                <div class="stat">
                    <div class="stat-value">{post_count}</div>
                    <div class="stat-label">Posts</div>
                </div>
                <div class="stat">
                    <div class="stat-value">0</div>
                    <div class="stat-label">Following</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{follower_count}</div>
                    <div class="stat-label">Followers</div>
                </div>
            </div>

            <div class="actions">
                <a href="/users/{outbox_username}/outbox" class="button button-primary">View Posts</a>
                <a href="{person_id}" class="button button-secondary">ActivityPub JSON</a>
            </div>
        </div>

        <div class="footer">
            <p>Powered by <a href="https://dais.social">dais</a> - Self-hosted ActivityPub</p>
        </div>
    </div>
</body>
</html>"#,
        // Light mode colors
        bg_primary = light.bg_primary,
        bg_secondary = light.bg_secondary,
        text_primary = light.text_primary,
        text_secondary = light.text_secondary,
        accent_primary = light.accent_primary,
        accent_hover = light.accent_hover,
        border = light.border,
        // Dark mode colors
        dark_bg_primary = dark.bg_primary,
        dark_bg_secondary = dark.bg_secondary,
        dark_text_primary = dark.text_primary,
        dark_text_secondary = dark.text_secondary,
        dark_accent_primary = dark.accent_primary,
        dark_accent_hover = dark.accent_hover,
        dark_border = dark.border,
        // Content
        avatar_initial = display_name.chars().next().unwrap_or('?').to_uppercase(),
        display_name = display_name,
        handle_username = username,
        bio_html = if !summary.is_empty() {
            format!(r#"<div class="bio">{}</div>"#, summary)
        } else {
            String::new()
        },
        post_count = post_count,
        follower_count = follower_count,
        outbox_username = username,
        person_id = person.id
    )
}
