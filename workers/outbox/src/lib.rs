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
    // Add CORS headers for federation
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;

    // Handle OPTIONS request
    if req.method() == Method::Options {
        return Ok(Response::empty()?.with_headers(headers));
    }

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

    // Build OrderedCollection
    let outbox_id = format!("https://social.dais.social/users/{}/outbox", username);
    let collection = OrderedCollection::new(outbox_id, notes);

    Ok(Response::from_json(&collection)?.with_headers(headers))
}

/// Handle GET /users/:username/posts/:id
/// Returns individual Note object
async fn handle_post(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Add CORS headers for federation
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;

    // Handle OPTIONS request
    if req.method() == Method::Options {
        return Ok(Response::empty()?.with_headers(headers));
    }

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

    // Construct full post ID (URL)
    let post_id = format!("https://social.dais.social/users/{}/posts/{}", username, post_id_param);

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

    Ok(Response::from_json(&note)?.with_headers(headers))
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
