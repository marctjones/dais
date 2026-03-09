use worker::*;
use shared::activitypub::{Note, OrderedCollection, Attachment, activitypub_context};
use shared::theme::Theme;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .options("/users/:username/outbox", |_req, _ctx| {
            let mut headers = Headers::new();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(Response::empty()?.with_headers(headers))
        })
        .options("/users/:username/posts/:id", |_req, _ctx| {
            let mut headers = Headers::new();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(Response::empty()?.with_headers(headers))
        })
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
        SELECT id, content, content_html, visibility, published_at, in_reply_to, media_attachments
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

        // Parse media attachments if present
        let attachments: Option<Vec<Attachment>> = post["media_attachments"]
            .as_str()
            .and_then(|s| {
                if s.is_empty() || s == "[]" {
                    None
                } else {
                    serde_json::from_str(s).ok()
                }
            });

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
            attachment: attachments,
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

    // Get theme from environment (default to "dais")
    let theme_name = ctx.env.var("THEME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais".to_string());
    let theme = Theme::from_name(&theme_name);

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let total_items = notes.len();
        let html = render_outbox_html(username, &notes, total_items, &theme);
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
               p.published_at, p.in_reply_to, p.media_attachments
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

    // Parse media attachments if present
    let attachments: Option<Vec<Attachment>> = post["media_attachments"]
        .as_str()
        .and_then(|s| {
            if s.is_empty() || s == "[]" {
                None
            } else {
                serde_json::from_str(s).ok()
            }
        });

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
        attachment: attachments.clone(),
    };

    // Get theme from environment (default to "dais")
    let theme_name = ctx.env.var("THEME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais".to_string());
    let theme = Theme::from_name(&theme_name);

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_post_html(username, &note, &attachments, &theme);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&note)?.with_headers(headers))
    }
}

fn render_post_html(username: &str, note: &Note, attachments: &Option<Vec<Attachment>>, theme: &Theme) -> String {
    let light = &theme.light;
    let dark = &theme.dark;

    // Build attachments HTML
    let attachments_html = if let Some(atts) = attachments {
        if !atts.is_empty() {
            let images: Vec<String> = atts.iter().filter(|a| a.attachment_type == "Image").map(|att| {
                let alt_text = att.name.as_ref().map(|s| s.as_str()).unwrap_or("");
                format!(r#"<img src="{}" alt="{}" loading="lazy">"#, att.url, alt_text)
            }).collect();

            if !images.is_empty() {
                format!(r#"<div class="attachments">{}</div>"#, images.join("\n"))
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Post by @{username}@dais.social</title>
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
        .post {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 32px;
            box-shadow: {shadow};
        }}
        .header {{
            display: flex;
            align-items: center;
            margin-bottom: 24px;
            padding-bottom: 20px;
            border-bottom: 1px solid {border};
        }}
        .avatar {{
            width: 56px;
            height: 56px;
            background: linear-gradient(135deg, {accent_primary} 0%, {accent_hover} 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 16px;
            font-size: 24px;
            color: white;
            font-weight: 700;
            flex-shrink: 0;
        }}
        .author {{
            flex: 1;
        }}
        .name {{
            font-weight: 600;
            font-size: 18px;
            color: {text_primary};
            margin-bottom: 2px;
        }}
        .handle {{
            color: {text_secondary};
            font-size: 15px;
        }}
        .content {{
            font-size: 17px;
            line-height: 1.7;
            margin-bottom: 24px;
            white-space: pre-wrap;
            word-wrap: break-word;
            color: {text_primary};
        }}
        .attachments {{
            margin: 24px 0;
            border-radius: 12px;
            overflow: hidden;
        }}
        .attachments img {{
            width: 100%;
            height: auto;
            display: block;
            margin-bottom: 8px;
            border-radius: 12px;
        }}
        .attachments img:last-child {{
            margin-bottom: 0;
        }}
        .meta {{
            color: {text_secondary};
            font-size: 15px;
            padding-top: 16px;
            border-top: 1px solid {border};
        }}
        .footer {{
            text-align: center;
            margin-top: 32px;
            padding-top: 24px;
            border-top: 1px solid {border};
            color: {text_secondary};
            font-size: 15px;
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
            .post {{
                background: {dark_bg_secondary};
            }}
            .name, .content {{
                color: {dark_text_primary};
            }}
            .handle, .meta {{
                color: {dark_text_secondary};
            }}
            .header, .meta {{
                border-color: {dark_border};
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
        <div class="post">
            <div class="header">
                <div class="avatar">{avatar_initial}</div>
                <div class="author">
                    <div class="name">@{name_username}</div>
                    <div class="handle">@{handle_username}@dais.social</div>
                </div>
            </div>
            <div class="content">{content}</div>
            {attachments}
            <div class="meta">
                Posted: {published}
            </div>
        </div>
        <div class="footer">
            <p><a href="/users/{outbox_username}/outbox">← Back to posts</a></p>
            <p style="margin-top: 10px;">Powered by <a href="https://dais.social">dais</a></p>
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
        shadow = light.shadow,
        // Dark mode colors
        dark_bg_primary = dark.bg_primary,
        dark_bg_secondary = dark.bg_secondary,
        dark_text_primary = dark.text_primary,
        dark_text_secondary = dark.text_secondary,
        dark_accent_hover = dark.accent_hover,
        dark_border = dark.border,
        // Content
        avatar_initial = username.chars().next().unwrap_or('?').to_uppercase(),
        name_username = username,
        handle_username = username,
        content = note.content,
        published = note.published,
        outbox_username = username,
        attachments = attachments_html
    )
}

fn render_outbox_html(username: &str, notes: &[serde_json::Value], total_items: usize, theme: &Theme) -> String {
    let light = &theme.light;
    let dark = &theme.dark;

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

            // Build attachment preview (show first image thumbnail)
            let attachment_preview = if let Some(attachments) = note["attachment"].as_array() {
                if let Some(first_image) = attachments.iter().find(|a| a["type"].as_str() == Some("Image")) {
                    if let Some(url) = first_image["url"].as_str() {
                        let alt = first_image["name"].as_str().unwrap_or("");
                        format!(r#"<div class="attachment-preview"><img src="{}" alt="{}" loading="lazy"></div>"#, url, alt)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            format!(r#"
            <div class="post">
                <div class="post-header">
                    <div class="avatar">{}</div>
                    <div class="author">
                        <div class="name">@{}</div>
                        <div class="timestamp">{}</div>
                    </div>
                </div>
                <div class="content">{}</div>
                {}
                <div class="actions">
                    <a href="/users/{}/posts/{}" class="view-link">View full post →</a>
                </div>
            </div>
            "#,
            username.chars().next().unwrap_or('?').to_uppercase(),
            username,
            published,
            preview,
            attachment_preview,
            username,
            post_id)
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
        .page-header {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 40px;
            margin-bottom: 24px;
            text-align: center;
        }}
        h1 {{
            font-size: 32px;
            font-weight: 700;
            margin-bottom: 8px;
            color: {text_primary};
        }}
        .subtitle {{
            color: {text_secondary};
            font-size: 17px;
        }}
        .post {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 28px;
            margin-bottom: 16px;
            transition: transform 0.2s ease, box-shadow 0.2s ease;
        }}
        .post:hover {{
            transform: translateY(-2px);
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
        }}
        .post-header {{
            display: flex;
            align-items: center;
            margin-bottom: 16px;
        }}
        .avatar {{
            width: 40px;
            height: 40px;
            background: linear-gradient(135deg, {accent_primary} 0%, #0F766E 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 12px;
            font-size: 18px;
            color: white;
            font-weight: 700;
            flex-shrink: 0;
        }}
        .author {{
            flex: 1;
        }}
        .name {{
            font-weight: 600;
            font-size: 16px;
            color: {text_primary};
        }}
        .timestamp {{
            color: {text_secondary};
            font-size: 14px;
            margin-top: 2px;
        }}
        .content {{
            font-size: 16px;
            line-height: 1.6;
            margin-bottom: 16px;
            white-space: pre-wrap;
            word-wrap: break-word;
            color: {text_primary};
        }}
        .attachment-preview {{
            margin: 16px 0;
            border-radius: 12px;
            overflow: hidden;
        }}
        .attachment-preview img {{
            width: 100%;
            height: auto;
            display: block;
            border-radius: 12px;
        }}
        .actions {{
            padding-top: 12px;
            border-top: 1px solid {border};
        }}
        .view-link {{
            color: {accent_hover};
            text-decoration: none;
            font-size: 15px;
            font-weight: 500;
        }}
        .view-link:hover {{
            color: {accent_primary};
            text-decoration: underline;
        }}
        .empty {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 60px 40px;
            text-align: center;
            color: {text_secondary};
            font-size: 17px;
        }}
        .footer {{
            text-align: center;
            margin-top: 32px;
            padding-top: 24px;
            border-top: 1px solid {border};
            color: {text_secondary};
            font-size: 15px;
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
            .page-header, .post, .empty {{
                background: {dark_bg_secondary};
            }}
            h1, .name, .content {{
                color: {dark_text_primary};
            }}
            .subtitle, .timestamp, .empty {{
                color: {dark_text_secondary};
            }}
            .actions {{
                border-top-color: {dark_border};
            }}
            .view-link {{
                color: {dark_accent_hover};
            }}
            .view-link:hover {{
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
        <div class="page-header">
            <h1>@{username}</h1>
            <div class="subtitle">{total_items} {post_word} · @{username}@dais.social</div>
        </div>
        {posts_html}
        <div class="footer">
            <p><a href="/users/{username}">View profile</a></p>
            <p style="margin-top: 10px;">Powered by <a href="https://dais.social">dais</a></p>
        </div>
    </div>
</body>
</html>"#,
        // Variables
        total_items = total_items,
        post_word = if total_items == 1 { "post" } else { "posts" },
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
