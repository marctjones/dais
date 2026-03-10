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

    // Fetch interactions for HTML rendering
    let mut replies: Vec<serde_json::Value> = Vec::new();
    let mut likes: Vec<serde_json::Value> = Vec::new();
    let mut boosts: Vec<serde_json::Value> = Vec::new();

    if wants_html {
        // Fetch replies to this post
        let replies_query = r#"
            SELECT actor_username, actor_display_name, actor_avatar_url, content, published_at
            FROM replies
            WHERE post_id = ?
            ORDER BY published_at ASC
        "#;
        let replies_stmt = db.prepare(replies_query).bind(&[post_id.clone().into()])?;
        let replies_result = replies_stmt.all().await?;
        replies = replies_result.results::<serde_json::Value>()?;

        // Fetch likes for this post
        let likes_query = r#"
            SELECT actor_username, actor_display_name, actor_avatar_url
            FROM interactions
            WHERE post_id = ? AND type = 'like'
            ORDER BY created_at DESC
        "#;
        let likes_stmt = db.prepare(likes_query).bind(&[post_id.clone().into()])?;
        let likes_result = likes_stmt.all().await?;
        likes = likes_result.results::<serde_json::Value>()?;

        // Fetch boosts for this post
        let boosts_query = r#"
            SELECT actor_username, actor_display_name, actor_avatar_url, created_at
            FROM interactions
            WHERE post_id = ? AND type = 'boost'
            ORDER BY created_at DESC
        "#;
        let boosts_stmt = db.prepare(boosts_query).bind(&[post_id.clone().into()])?;
        let boosts_result = boosts_stmt.all().await?;
        boosts = boosts_result.results::<serde_json::Value>()?;
    }

    // Get theme from environment (default to "dais")
    let theme_name = ctx.env.var("THEME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais".to_string());
    let theme = Theme::from_name(&theme_name);

    // Return HTML or JSON based on Accept header
    if wants_html {
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        let html = render_post_html(username, &note, &attachments, &replies, &likes, &boosts, &theme);
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        Ok(Response::from_json(&note)?.with_headers(headers))
    }
}

/// Convert external video URLs to embeds
fn embed_external_videos(content: &str) -> String {
    let mut result = content.to_string();

    // YouTube patterns
    let youtube_patterns = [
        (r"https?://(?:www\.)?youtube\.com/watch\?v=([a-zA-Z0-9_-]+)", "https://www.youtube.com/embed/$1"),
        (r"https?://youtu\.be/([a-zA-Z0-9_-]+)", "https://www.youtube.com/embed/$1"),
    ];

    // Vimeo pattern
    let vimeo_pattern = (r"https?://(?:www\.)?vimeo\.com/(\d+)", "https://player.vimeo.com/video/$1");

    // Replace YouTube URLs
    for (_pattern, _embed_template) in &youtube_patterns {
        // Simple regex-like replacement (Rust doesn't have regex in wasm by default)
        // This is a simplified version - would need proper regex crate for production
        if result.contains("youtube.com/watch?v=") || result.contains("youtu.be/") {
            // Extract video ID manually
            if let Some(start) = result.find("youtube.com/watch?v=") {
                let id_start = start + 20; // Length of "youtube.com/watch?v="
                if let Some(id_end) = result[id_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                    let video_id = &result[id_start..id_start + id_end];
                    let embed_html = format!(
                        r#"<div class="video-embed"><iframe src="https://www.youtube.com/embed/{}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe></div>"#,
                        video_id
                    );
                    result = result.replace(&format!("https://www.youtube.com/watch?v={}", video_id), &embed_html);
                } else {
                    // ID goes to end of string
                    let video_id = &result[id_start..];
                    let embed_html = format!(
                        r#"<div class="video-embed"><iframe src="https://www.youtube.com/embed/{}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe></div>"#,
                        video_id
                    );
                    result = result.replace(&format!("https://www.youtube.com/watch?v={}", video_id), &embed_html);
                }
            } else if let Some(start) = result.find("youtu.be/") {
                let id_start = start + 9; // Length of "youtu.be/"
                if let Some(id_end) = result[id_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                    let video_id = &result[id_start..id_start + id_end];
                    let embed_html = format!(
                        r#"<div class="video-embed"><iframe src="https://www.youtube.com/embed/{}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe></div>"#,
                        video_id
                    );
                    result = result.replace(&format!("https://youtu.be/{}", video_id), &embed_html);
                } else {
                    let video_id = &result[id_start..];
                    let embed_html = format!(
                        r#"<div class="video-embed"><iframe src="https://www.youtube.com/embed/{}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe></div>"#,
                        video_id
                    );
                    result = result.replace(&format!("https://youtu.be/{}", video_id), &embed_html);
                }
            }
        }
    }

    // Replace Vimeo URLs
    if result.contains("vimeo.com/") {
        if let Some(start) = result.find("vimeo.com/") {
            let id_start = start + 10; // Length of "vimeo.com/"
            if let Some(id_end) = result[id_start..].find(|c: char| !c.is_numeric()) {
                let video_id = result[id_start..id_start + id_end].to_string();
                let embed_html = format!(
                    r#"<div class="video-embed"><iframe src="https://player.vimeo.com/video/{}" frameborder="0" allow="autoplay; fullscreen; picture-in-picture" allowfullscreen></iframe></div>"#,
                    video_id
                );
                result = result.replace(&format!("https://vimeo.com/{}", video_id), &embed_html);
                result = result.replace(&format!("https://www.vimeo.com/{}", video_id), &embed_html);
            }
        }
    }

    result
}

fn render_post_html(username: &str, note: &Note, attachments: &Option<Vec<Attachment>>, replies: &[serde_json::Value], likes: &[serde_json::Value], boosts: &[serde_json::Value], theme: &Theme) -> String {
    let light = &theme.light;
    let dark = &theme.dark;

    // Process content for external video embeds
    let processed_content = embed_external_videos(&note.content);

    // Build interaction counts HTML
    let like_count = likes.len();
    let boost_count = boosts.len();
    let reply_count = replies.len();

    let interaction_counts_html = if like_count > 0 || boost_count > 0 || reply_count > 0 {
        format!(r#"
        <div class="interaction-counts">
            {}{}{}
        </div>
        "#,
            if reply_count > 0 {
                format!(r#"<span class="count">💬 {} {}</span>"#, reply_count, if reply_count == 1 { "reply" } else { "replies" })
            } else { String::new() },
            if boost_count > 0 {
                format!(r#"<span class="count">🔁 {} {}</span>"#, boost_count, if boost_count == 1 { "boost" } else { "boosts" })
            } else { String::new() },
            if like_count > 0 {
                format!(r#"<span class="count">❤️ {} {}</span>"#, like_count, if like_count == 1 { "like" } else { "likes" })
            } else { String::new() }
        )
    } else {
        String::new()
    };

    // Build replies HTML
    let replies_html = if !replies.is_empty() {
        let replies_items: Vec<String> = replies.iter().map(|reply| {
            let actor_username = reply["actor_username"].as_str().unwrap_or("unknown");
            let actor_display_name = reply["actor_display_name"].as_str().unwrap_or(actor_username);
            let content = reply["content"].as_str().unwrap_or("");
            let published_at = reply["published_at"].as_str().unwrap_or("");
            let avatar_initial = actor_username.chars().next().unwrap_or('?').to_uppercase();

            format!(r#"
            <div class="reply">
                <div class="reply-header">
                    <div class="reply-avatar">{}</div>
                    <div class="reply-author">
                        <div class="reply-name">{}</div>
                        <div class="reply-handle">{}</div>
                    </div>
                    <div class="reply-timestamp">{}</div>
                </div>
                <div class="reply-content">{}</div>
            </div>
            "#, avatar_initial, actor_display_name, actor_username, published_at, content)
        }).collect();

        format!(r#"
        <div class="replies-section">
            <h3 class="replies-title">Replies</h3>
            {}
        </div>
        "#, replies_items.join("\n"))
    } else {
        String::new()
    };

    // Build attachments HTML
    let attachments_html = if let Some(atts) = attachments {
        if !atts.is_empty() {
            let mut media_items: Vec<String> = Vec::new();

            // Render images
            for att in atts.iter().filter(|a| a.attachment_type == "Image") {
                let alt_text = att.name.as_ref().map(|s| s.as_str()).unwrap_or("");
                media_items.push(format!(r#"<img src="{}" alt="{}" loading="lazy">"#, att.url, alt_text));
            }

            // Render videos
            for att in atts.iter().filter(|a| a.attachment_type == "Video") {
                let alt_text = att.name.as_ref().map(|s| s.as_str()).unwrap_or("");
                media_items.push(format!(
                    r#"<video controls preload="metadata" aria-label="{}"><source src="{}" type="{}">Your browser does not support video playback.</video>"#,
                    alt_text, att.url, att.media_type
                ));
            }

            if !media_items.is_empty() {
                format!(r#"<div class="attachments">{}</div>"#, media_items.join("\n"))
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
        .attachments video {{
            width: 100%;
            height: auto;
            display: block;
            margin-bottom: 8px;
            border-radius: 12px;
            background: #000;
        }}
        .attachments img:last-child,
        .attachments video:last-child {{
            margin-bottom: 0;
        }}
        .video-embed {{
            margin: 24px 0;
            position: relative;
            padding-bottom: 56.25%; /* 16:9 aspect ratio */
            height: 0;
            overflow: hidden;
            border-radius: 12px;
        }}
        .video-embed iframe {{
            position: absolute;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            border: 0;
            border-radius: 12px;
        }}
        .meta {{
            color: {text_secondary};
            font-size: 15px;
            padding-top: 16px;
            border-top: 1px solid {border};
        }}
        .interaction-counts {{
            display: flex;
            gap: 20px;
            padding: 16px 0;
            margin-top: 16px;
            border-top: 1px solid {border};
            font-size: 15px;
            color: {text_secondary};
        }}
        .interaction-counts .count {{
            display: inline-flex;
            align-items: center;
            gap: 6px;
        }}
        .replies-section {{
            margin-top: 24px;
            padding-top: 24px;
            border-top: 2px solid {border};
        }}
        .replies-title {{
            font-size: 20px;
            font-weight: 600;
            color: {text_primary};
            margin-bottom: 20px;
        }}
        .reply {{
            background: {bg_primary};
            border-radius: 12px;
            padding: 20px;
            margin-bottom: 16px;
        }}
        .reply:last-child {{
            margin-bottom: 0;
        }}
        .reply-header {{
            display: flex;
            align-items: center;
            margin-bottom: 12px;
            gap: 12px;
        }}
        .reply-avatar {{
            width: 40px;
            height: 40px;
            background: linear-gradient(135deg, {accent_primary} 0%, {accent_hover} 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 18px;
            color: white;
            font-weight: 600;
            flex-shrink: 0;
        }}
        .reply-author {{
            flex: 1;
        }}
        .reply-name {{
            font-weight: 600;
            font-size: 15px;
            color: {text_primary};
        }}
        .reply-handle {{
            color: {text_secondary};
            font-size: 14px;
        }}
        .reply-timestamp {{
            color: {text_secondary};
            font-size: 14px;
        }}
        .reply-content {{
            font-size: 15px;
            line-height: 1.6;
            color: {text_primary};
            white-space: pre-wrap;
            word-wrap: break-word;
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
            .header, .meta, .interaction-counts, .replies-section {{
                border-color: {dark_border};
            }}
            .interaction-counts {{
                color: {dark_text_secondary};
            }}
            .replies-title, .reply-name, .reply-content {{
                color: {dark_text_primary};
            }}
            .reply-handle, .reply-timestamp {{
                color: {dark_text_secondary};
            }}
            .reply {{
                background: {dark_bg_primary};
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
            {interaction_counts}
            {replies}
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
        content = processed_content,
        published = note.published,
        outbox_username = username,
        attachments = attachments_html,
        interaction_counts = interaction_counts_html,
        replies = replies_html
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

            // Process content for external video embeds
            let processed_content = embed_external_videos(content);

            // Truncate long posts (after embed processing)
            let preview = if processed_content.len() > 280 {
                format!("{}...", &processed_content[..280])
            } else {
                processed_content.to_string()
            };

            // Build attachment preview (show first media item)
            let attachment_preview = if let Some(attachments) = note["attachment"].as_array() {
                // Try to find first image
                if let Some(first_image) = attachments.iter().find(|a| a["type"].as_str() == Some("Image")) {
                    if let Some(url) = first_image["url"].as_str() {
                        let alt = first_image["name"].as_str().unwrap_or("");
                        format!(r#"<div class="attachment-preview"><img src="{}" alt="{}" loading="lazy"></div>"#, url, alt)
                    } else {
                        String::new()
                    }
                // Try to find first video
                } else if let Some(first_video) = attachments.iter().find(|a| a["type"].as_str() == Some("Video")) {
                    if let Some(url) = first_video["url"].as_str() {
                        let media_type = first_video["mediaType"].as_str().unwrap_or("video/mp4");
                        let alt = first_video["name"].as_str().unwrap_or("");
                        format!(
                            r#"<div class="attachment-preview"><video controls preload="metadata" aria-label="{}"><source src="{}" type="{}"></video></div>"#,
                            alt, url, media_type
                        )
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
        .attachment-preview video {{
            width: 100%;
            height: auto;
            display: block;
            border-radius: 12px;
            background: #000;
        }}
        .video-embed {{
            margin: 16px 0;
            position: relative;
            padding-bottom: 56.25%; /* 16:9 aspect ratio */
            height: 0;
            overflow: hidden;
            border-radius: 12px;
        }}
        .video-embed iframe {{
            position: absolute;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            border: 0;
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
