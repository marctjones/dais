use worker::*;
use shared::activitypub::{Note, OrderedCollection, activitypub_context};

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/users/:username/outbox", handle_outbox)
        .get_async("/users/:username/posts/:id", handle_post)
        .run(req, env)
        .await
}

/// Handle GET /users/:username/outbox
/// Returns OrderedCollection of all posts by this user
async fn handle_outbox(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        let headers = Headers::new();
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

    // Verify actor exists
    let actor_query = "SELECT id FROM actors WHERE username = ?";
    let actor_stmt = db.prepare(actor_query).bind(&[username.into()])?;
    let actor_result = actor_stmt.first::<serde_json::Value>(None).await?;

    let actor_id = match actor_result {
        Some(data) => data["id"].as_str().ok_or("Missing actor id")?.to_string(),
        None => {
            console_log!("Actor not found: {}", username);
            return Response::error("Actor not found", 404);
        }
    };

    // Query for posts by this actor (public visibility only for outbox)
    // Order by published_at DESC for reverse chronological
    let posts_query = r#"
        SELECT id, content, content_html, visibility, published_at, in_reply_to
        FROM posts
        WHERE actor_id = ? AND visibility IN ('public', 'unlisted')
        ORDER BY published_at DESC
    "#;

    let posts_stmt = db.prepare(posts_query).bind(&[actor_id.clone().into()])?;
    let posts_result = posts_stmt.all().await?;

    let posts = posts_result.results::<serde_json::Value>()?;

    // Convert posts to Note objects
    let mut notes = Vec::new();
    for post in posts {
        let post_id = post["id"].as_str().unwrap_or("");
        let content = post["content"].as_str().unwrap_or("");
        let published = post["published_at"].as_str().unwrap_or("");

        let note = Note {
            context: activitypub_context(),
            note_type: "Note".to_string(),
            id: post_id.to_string(),
            attributed_to: actor_id.clone(),
            content: content.to_string(),
            published: published.to_string(),
            to: vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
            cc: None,
            in_reply_to: post["in_reply_to"].as_str().map(|s| s.to_string()),
            attachment: None,
        };

        notes.push(serde_json::to_value(note)?);
    }

    // Get ActivityPub domain from environment
    let activitypub_domain = ctx.env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    // Build OrderedCollection
    let outbox_id = format!("https://{}/users/{}/outbox", activitypub_domain, username);
    let collection = OrderedCollection::new(outbox_id.clone(), notes.clone());

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_outbox_html(username, &notes);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&collection)?.with_headers(headers))
    }
}

/// Handle GET /users/:username/posts/:id
/// Returns individual Note object
async fn handle_post(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        let headers = Headers::new();
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

    // Get username and post ID from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    let post_id_param = match ctx.param("id") {
        Some(i) => i,
        None => return Response::error("Post ID required", 400),
    };

    // Get D1 database
    let db = ctx.env.d1("DB").expect("D1 database binding not found");

    // Get ActivityPub domain from environment
    let activitypub_domain = ctx.env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    // Construct full post ID (URL)
    let post_id = format!("https://{}/users/{}/posts/{}", activitypub_domain, username, post_id_param);

    // Query for post
    let post_query = r#"
        SELECT p.id, p.actor_id, p.content, p.content_html, p.visibility,
               p.published_at, p.in_reply_to
        FROM posts p
        JOIN actors a ON p.actor_id = a.id
        WHERE p.id = ? AND a.username = ?
    "#;

    let stmt = db.prepare(post_query).bind(&[post_id.clone().into(), username.into()])?;
    let result = stmt.first::<serde_json::Value>(None).await?;

    let post = match result {
        Some(data) => data,
        None => {
            console_log!("Post not found: {}", post_id);
            return Response::error("Post not found", 404);
        }
    };

    // Build Note object
    let note = Note {
        context: activitypub_context(),
        note_type: "Note".to_string(),
        id: post["id"].as_str().unwrap_or("").to_string(),
        attributed_to: post["actor_id"].as_str().unwrap_or("").to_string(),
        content: post["content"].as_str().unwrap_or("").to_string(),
        published: post["published_at"].as_str().unwrap_or("").to_string(),
        to: vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
        cc: None,
        in_reply_to: post["in_reply_to"].as_str().map(|s| s.to_string()),
        attachment: None,
    };

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_post_html(username, &note);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&note)?.with_headers(headers))
    }
}

fn render_post_html(username: &str, note: &Note) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Post by @{username}@dais.social</title>
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
        }}
        .post {{
            background: #2a2a2a;
            border-radius: 12px;
            padding: 30px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
        }}
        .header {{
            display: flex;
            align-items: center;
            margin-bottom: 20px;
            padding-bottom: 15px;
            border-bottom: 1px solid #3a3a3a;
        }}
        .avatar {{
            width: 48px;
            height: 48px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 12px;
            font-size: 20px;
            color: white;
            flex-shrink: 0;
        }}
        .author {{
            flex: 1;
        }}
        .name {{
            font-weight: 600;
            color: #ffffff;
        }}
        .handle {{
            color: #8899a6;
            font-size: 14px;
        }}
        .content {{
            font-size: 16px;
            line-height: 1.6;
            margin-bottom: 20px;
            white-space: pre-wrap;
            word-wrap: break-word;
        }}
        .meta {{
            color: #8899a6;
            font-size: 14px;
            padding-top: 15px;
            border-top: 1px solid #3a3a3a;
        }}
        .footer {{
            text-align: center;
            margin-top: 30px;
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
        <div class="post">
            <div class="header">
                <div class="avatar">{}</div>
                <div class="author">
                    <div class="name">@{}</div>
                    <div class="handle">@{}@dais.social</div>
                </div>
            </div>
            <div class="content">{}</div>
            <div class="meta">
                Posted: {}
            </div>
        </div>
        <div class="footer">
            <p><a href="/users/{}/outbox">← Back to posts</a></p>
            <p style="margin-top: 10px;">Powered by <a href="https://dais.social">dais</a></p>
        </div>
    </div>
</body>
</html>"#,
        username.chars().next().unwrap_or('?').to_uppercase(),
        username,
        username,
        note.content,
        note.published,
        username
    )
}

fn render_outbox_html(username: &str, notes: &[serde_json::Value]) -> String {
    let posts_html = if notes.is_empty() {
        r#"<div class="empty">No posts yet.</div>"#.to_string()
    } else {
        notes.iter().map(|note| {
            let content = note["content"].as_str().unwrap_or("");
            let published = note["published"].as_str().unwrap_or("");
            let id = note["id"].as_str().unwrap_or("");
            let post_id = id.split('/').last().unwrap_or("");

            // Truncate long posts
            let preview = if content.len() > 280 {
                format!("{}...", &content[..280])
            } else {
                content.to_string()
            };

            format!(r#"
            <div class="post">
                <div class="content">{}</div>
                <div class="meta">
                    {} · <a href="/users/{}/posts/{}">View post</a>
                </div>
            </div>
            "#, preview, published, username, post_id)
        }).collect::<Vec<_>>().join("\n")
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>@{username}@dais.social - Posts</title>
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
        }}
        .header {{
            background: #2a2a2a;
            border-radius: 12px;
            padding: 30px;
            margin-bottom: 20px;
            text-align: center;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.3);
        }}
        h1 {{
            font-size: 28px;
            margin-bottom: 8px;
        }}
        .subtitle {{
            color: #8899a6;
            font-size: 16px;
        }}
        .post {{
            background: #2a2a2a;
            border-radius: 12px;
            padding: 25px;
            margin-bottom: 15px;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
        }}
        .content {{
            font-size: 16px;
            line-height: 1.6;
            margin-bottom: 15px;
            white-space: pre-wrap;
            word-wrap: break-word;
        }}
        .meta {{
            color: #8899a6;
            font-size: 14px;
            padding-top: 12px;
            border-top: 1px solid #3a3a3a;
        }}
        .meta a {{
            color: #667eea;
            text-decoration: none;
        }}
        .meta a:hover {{
            text-decoration: underline;
        }}
        .empty {{
            background: #2a2a2a;
            border-radius: 12px;
            padding: 40px;
            text-align: center;
            color: #8899a6;
        }}
        .footer {{
            text-align: center;
            margin-top: 30px;
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
            <h1>@{username}</h1>
            <div class="subtitle">@{username}@dais.social</div>
        </div>
        {posts_html}
        <div class="footer">
            <p><a href="/users/{username}">View profile</a></p>
            <p style="margin-top: 10px;">Powered by <a href="https://dais.social">dais</a></p>
        </div>
    </div>
</body>
</html>"#,
        username = username,
        posts_html = posts_html
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_id_format() {
        let username = "marc";
        let post_id_param = "001";
        let expected = "https://social.dais.social/users/marc/posts/001";
        let actual = format!("https://social.dais.social/users/{}/posts/{}", username, post_id_param);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_outbox_id_format() {
        let username = "alice";
        let expected = "https://social.dais.social/users/alice/outbox";
        let actual = format!("https://social.dais.social/users/{}/outbox", username);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_ordered_collection_new() {
        let outbox_id = "https://social.dais.social/users/marc/outbox".to_string();
        let notes = vec![
            serde_json::json!({"type": "Note", "content": "Test 1"}),
            serde_json::json!({"type": "Note", "content": "Test 2"}),
        ];

        let collection = OrderedCollection::new(outbox_id.clone(), notes);

        assert_eq!(collection.collection_type, "OrderedCollection");
        assert_eq!(collection.id, outbox_id);
        assert_eq!(collection.total_items, 2);
        assert!(collection.ordered_items.is_some());
        assert_eq!(collection.ordered_items.unwrap().len(), 2);
    }

    #[test]
    fn test_ordered_collection_empty() {
        let outbox_id = "https://social.dais.social/users/marc/outbox".to_string();
        let collection = OrderedCollection::empty(outbox_id.clone());

        assert_eq!(collection.collection_type, "OrderedCollection");
        assert_eq!(collection.id, outbox_id);
        assert_eq!(collection.total_items, 0);
        assert!(collection.ordered_items.is_some());
        assert_eq!(collection.ordered_items.unwrap().len(), 0);
    }
}
