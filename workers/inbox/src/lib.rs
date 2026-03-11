use worker::*;
use serde::Deserialize;
use shared::crypto::HttpSignature;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(Debug, Deserialize)]
struct Activity {
    #[serde(rename = "type")]
    activity_type: String,
    id: String,
    actor: String,
    object: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ActorObject {
    #[serde(rename = "publicKey")]
    public_key: PublicKeyObject,
}

#[derive(Debug, Deserialize)]
struct PublicKeyObject {
    #[serde(rename = "publicKeyPem")]
    public_key_pem: String,
}

/// Fetch an actor's public key from their ActivityPub profile
async fn fetch_actor_public_key(actor_url: &str) -> Result<String> {
    console_log!("Fetching actor from: {}", actor_url);

    // Fetch the actor's profile with ActivityPub content-type
    let headers = worker::Headers::new();
    headers.set("Accept", "application/activity+json")?;

    let mut request_init = RequestInit::new();
    request_init.with_method(worker::Method::Get);
    request_init.headers = headers;

    let request = worker::Request::new_with_init(actor_url, &request_init)?;
    let mut response = worker::Fetch::Request(request).send().await?;

    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Err(worker::Error::RustError(format!(
            "Failed to fetch actor: HTTP {}",
            status
        )));
    }

    let actor_json = response.text().await?;
    console_log!("Actor response: {}", &actor_json[..std::cmp::min(200, actor_json.len())]);

    // Parse the actor object
    let actor: ActorObject = serde_json::from_str(&actor_json)
        .map_err(|e| worker::Error::RustError(format!("Failed to parse actor JSON: {}", e)))?;

    Ok(actor.public_key.public_key_pem)
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .options("/users/:username/inbox", |_req, _ctx| {
            let headers = Headers::new();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "POST, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Content-Type, Signature, Date, Digest")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(Response::empty()?.with_headers(headers))
        })
        .post_async("/users/:username/inbox", handle_inbox)
        .run(req, env)
        .await
}

async fn handle_inbox(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    console_log!("Received activity for user: {}", username);

    // Get the request body
    let body = req.text().await?;
    console_log!("Activity body: {}", body);

    // Parse the activity
    let activity: Activity = match serde_json::from_str(&body) {
        Ok(a) => a,
        Err(e) => {
            console_log!("Failed to parse activity: {}", e);
            return Response::error("Invalid activity", 400);
        }
    };

    console_log!("Activity type: {}, actor: {}", activity.activity_type, activity.actor);

    // Verify HTTP signature
    let signature_header = match req.headers().get("Signature")? {
        Some(sig) => sig,
        None => {
            console_log!("Missing Signature header");
            return Response::error("Missing signature", 401);
        }
    };

    console_log!("Signature header: {}", signature_header);

    // Parse signature
    let http_signature = match HttpSignature::parse(&signature_header) {
        Ok(sig) => sig,
        Err(e) => {
            console_log!("Failed to parse signature: {}", e);
            return Response::error("Invalid signature format", 400);
        }
    };

    // Fetch the actor's public key
    console_log!("Fetching public key from: {}", activity.actor);
    let public_key_pem = match fetch_actor_public_key(&activity.actor).await {
        Ok(key) => key,
        Err(e) => {
            console_log!("Failed to fetch actor public key: {}", e);
            // For now, log the error but continue processing
            // In production, you might want to reject unsigned requests
            console_log!("WARNING: Proceeding without signature verification");
            String::new()
        }
    };

    // Verify the signature if we got the public key
    if !public_key_pem.is_empty() {
        console_log!("Verifying HTTP signature...");

        // First, verify the Digest header if present
        if let Some(digest_header) = req.headers().get("Digest")? {
            use sha2::{Sha256, Digest as Sha2Digest};

            let body_hash = Sha256::digest(body.as_bytes());
            let expected_digest = format!("SHA-256={}", BASE64.encode(&body_hash));

            if digest_header != expected_digest {
                console_log!("Digest mismatch: expected {}, got {}", expected_digest, digest_header);
                return Response::error("Invalid digest", 400);
            }
            console_log!("✓ Digest verified");
        }

        // Build headers map for signature verification
        let mut headers_map = std::collections::HashMap::new();
        if let Some(host) = req.headers().get("Host")? {
            headers_map.insert("host".to_string(), host);
        }
        if let Some(date) = req.headers().get("Date")? {
            headers_map.insert("date".to_string(), date);
        }
        if let Some(digest) = req.headers().get("Digest")? {
            headers_map.insert("digest".to_string(), digest);
        }
        if let Some(content_type) = req.headers().get("Content-Type")? {
            headers_map.insert("content-type".to_string(), content_type);
        }

        // Get request path
        let url = req.url()?;
        let path = url.path();

        // Verify signature
        let verified = shared::crypto::verify_request(
            &public_key_pem,
            &http_signature,
            "POST",
            path,
            &headers_map,
        ).map_err(|e| {
            console_log!("Signature verification error: {}", e);
            worker::Error::RustError(format!("Signature verification failed: {}", e))
        })?;

        if !verified {
            console_log!("✗ Signature verification failed");
            return Response::error("Invalid signature", 401);
        }
        console_log!("✓ Signature verified successfully");
    } else {
        console_log!("WARNING: Proceeding without signature verification (no public key)");
    }

    // Get D1 database
    let db = ctx.env.d1("DB").expect("D1 database binding not found");

    // Check if actor is blocked
    if is_blocked(&db, &activity.actor).await? {
        console_log!("Activity rejected: actor is blocked: {}", activity.actor);
        return Ok(Response::empty()?.with_status(403));
    }

    // Handle different activity types
    match activity.activity_type.as_str() {
        "Follow" => handle_follow(&db, &activity, username).await?,
        "Undo" => handle_undo(&db, &activity).await?,
        "Create" => handle_create(&db, &activity, username, &ctx).await?,
        "Like" => handle_like(&db, &activity, username).await?,
        "Announce" => handle_announce(&db, &activity, username).await?,
        "Accept" => handle_accept(&db, &activity).await?,
        "Reject" => handle_reject(&db, &activity).await?,
        _ => {
            console_log!("Unsupported activity type: {}", activity.activity_type);
        }
    }

    // Return 202 Accepted
    Ok(Response::empty()?.with_status(202))
}

async fn handle_follow(db: &D1Database, activity: &Activity, username: &str) -> Result<()> {
    console_log!("Processing Follow from: {}", activity.actor);

    // Extract follower's inbox from their actor object
    // For now, we'll store a placeholder inbox URL
    let follower_inbox = format!("{}/inbox", activity.actor);

    // Build our actor URL from username parameter
    // TODO: Get ActivityPub domain from environment variable
    let our_actor_url = format!("https://social.dais.social/users/{}", username);

    // Insert into followers table with 'pending' status
    let query = r#"
        INSERT OR IGNORE INTO followers (
            id, actor_id, follower_actor_id, follower_inbox, status
        ) VALUES (?, ?, ?, ?, 'pending')
    "#;

    let statement = db.prepare(query).bind(&[
        activity.id.as_str().into(),
        our_actor_url.as_str().into(),
        activity.actor.as_str().into(),
        follower_inbox.as_str().into(),
    ])?;

    statement.run().await?;

    console_log!("Stored follow request from: {}", activity.actor);
    console_log!("Status: pending (use CLI to approve/reject)");

    Ok(())
}

async fn handle_undo(db: &D1Database, activity: &Activity) -> Result<()> {
    console_log!("Processing Undo from: {}", activity.actor);

    // The object should be the Follow activity being undone
    if let Some(object_type) = activity.object.get("type").and_then(|v| v.as_str()) {
        if object_type == "Follow" {
            // Remove the follower
            let query = "DELETE FROM followers WHERE follower_actor_id = ?";
            let statement = db.prepare(query).bind(&[activity.actor.as_str().into()])?;
            statement.run().await?;

            console_log!("Removed follower: {}", activity.actor);
        } else if object_type == "Like" {
            // Remove the like
            if let Some(object_id) = activity.object.get("id").and_then(|v| v.as_str()) {
                let query = "DELETE FROM interactions WHERE id = ?";
                let statement = db.prepare(query).bind(&[object_id.into()])?;
                statement.run().await?;

                console_log!("Removed like: {}", object_id);
            }
        } else if object_type == "Announce" {
            // Remove the boost
            if let Some(object_id) = activity.object.get("id").and_then(|v| v.as_str()) {
                let query = "DELETE FROM interactions WHERE id = ?";
                let statement = db.prepare(query).bind(&[object_id.into()])?;
                statement.run().await?;

                console_log!("Removed boost: {}", object_id);
            }
        }
    }

    Ok(())
}

async fn handle_create(db: &D1Database, activity: &Activity, _username: &str, ctx: &RouteContext<()>) -> Result<()> {
    console_log!("Processing Create from: {}", activity.actor);

    // Check if the object is a Note (post/reply)
    if let Some(object_type) = activity.object.get("type").and_then(|v| v.as_str()) {
        if object_type == "Note" {
            // Check if this is a reply to one of our posts
            if let Some(in_reply_to) = activity.object.get("inReplyTo").and_then(|v| v.as_str()) {
                console_log!("This is a reply to: {}", in_reply_to);

                // Extract reply details
                let reply_id = activity.object.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&activity.id);

                let content = activity.object.get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let published_at = activity.object.get("published")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Fetch actor info for display
                let (actor_username, actor_display_name, actor_avatar_url) =
                    extract_actor_info(&activity.actor).await;

                // Check if in_reply_to is one of our posts
                let our_post_query = "SELECT id FROM posts WHERE id = ?";
                let our_post_stmt = db.prepare(our_post_query).bind(&[in_reply_to.into()])?;
                let our_post_result = our_post_stmt.first::<serde_json::Value>(None).await?;

                if our_post_result.is_some() {
                    console_log!("Reply is to one of our posts, moderating content...");

                    // Run AI moderation
                    let (moderation_status, moderation_score, moderation_flags, hidden) =
                        moderate_content(content, ctx).await?;

                    console_log!("Moderation result: status={}, score={:.2}, hidden={}",
                        moderation_status, moderation_score, hidden);

                    // Store the reply with moderation data
                    let insert_query = r#"
                        INSERT OR IGNORE INTO replies (
                            id, post_id, actor_id, actor_username, actor_display_name,
                            actor_avatar_url, content, published_at,
                            moderation_status, moderation_score, moderation_flags,
                            moderation_checked_at, hidden
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#;

                    let checked_at = chrono::Utc::now().to_rfc3339();

                    let statement = db.prepare(insert_query).bind(&[
                        reply_id.into(),
                        in_reply_to.into(),
                        activity.actor.as_str().into(),
                        actor_username.clone().into(),
                        actor_display_name.clone().into(),
                        actor_avatar_url.clone().into(),
                        content.into(),
                        published_at.into(),
                        moderation_status.as_str().into(),
                        moderation_score.into(),
                        moderation_flags.as_str().into(),
                        checked_at.as_str().into(),
                        hidden.into(),
                    ])?;

                    statement.run().await?;

                    // Create notification only if not hidden
                    if !hidden {
                        create_notification(
                            db,
                            "reply",
                            &activity.actor,
                            &actor_username,
                            &actor_display_name,
                            &actor_avatar_url,
                            Some(in_reply_to),
                            Some(reply_id),
                            Some(content),
                        ).await?;
                    } else {
                        console_log!("Reply auto-hidden by moderation, notification suppressed");
                    }

                    console_log!("Stored reply from: {}", activity.actor);
                }
            }
        }
    }

    Ok(())
}

async fn handle_like(db: &D1Database, activity: &Activity, _username: &str) -> Result<()> {
    console_log!("Processing Like from: {}", activity.actor);

    // Get the object being liked
    let object_url = if let Some(obj_str) = activity.object.as_str() {
        obj_str
    } else if let Some(obj_id) = activity.object.get("id").and_then(|v| v.as_str()) {
        obj_id
    } else {
        console_log!("Could not extract object URL from Like");
        return Ok(());
    };

    console_log!("Like object: {}", object_url);

    // Check if this is one of our posts
    let our_post_query = "SELECT id FROM posts WHERE id = ?";
    let our_post_stmt = db.prepare(our_post_query).bind(&[object_url.into()])?;
    let our_post_result = our_post_stmt.first::<serde_json::Value>(None).await?;

    if our_post_result.is_some() {
        console_log!("Like is for one of our posts, storing it");

        // Fetch actor info for display
        let (actor_username, actor_display_name, actor_avatar_url) =
            extract_actor_info(&activity.actor).await;

        // Get current timestamp
        let created_at = chrono::Utc::now().to_rfc3339();

        // Store the like
        let insert_query = r#"
            INSERT OR IGNORE INTO interactions (
                id, type, actor_id, actor_username, actor_display_name,
                actor_avatar_url, post_id, created_at
            ) VALUES (?, 'like', ?, ?, ?, ?, ?, ?)
        "#;

        let statement = db.prepare(insert_query).bind(&[
            activity.id.as_str().into(),
            activity.actor.as_str().into(),
            actor_username.clone().into(),
            actor_display_name.clone().into(),
            actor_avatar_url.clone().into(),
            object_url.into(),
            created_at.as_str().into(),
        ])?;

        statement.run().await?;

        // Create notification
        create_notification(
            db,
            "like",
            &activity.actor,
            &actor_username,
            &actor_display_name,
            &actor_avatar_url,
            Some(object_url),
            Some(&activity.id),
            None,
        ).await?;

        console_log!("Stored like from: {}", activity.actor);
    }

    Ok(())
}

async fn handle_announce(db: &D1Database, activity: &Activity, _username: &str) -> Result<()> {
    console_log!("Processing Announce (boost) from: {}", activity.actor);

    // Get the object being announced
    let object_url = if let Some(obj_str) = activity.object.as_str() {
        obj_str
    } else if let Some(obj_id) = activity.object.get("id").and_then(|v| v.as_str()) {
        obj_id
    } else {
        console_log!("Could not extract object URL from Announce");
        return Ok(());
    };

    console_log!("Announce object: {}", object_url);

    // Check if this is one of our posts
    let our_post_query = "SELECT id FROM posts WHERE id = ?";
    let our_post_stmt = db.prepare(our_post_query).bind(&[object_url.into()])?;
    let our_post_result = our_post_stmt.first::<serde_json::Value>(None).await?;

    if our_post_result.is_some() {
        console_log!("Boost is for one of our posts, storing it");

        // Fetch actor info for display
        let (actor_username, actor_display_name, actor_avatar_url) =
            extract_actor_info(&activity.actor).await;

        // Get current timestamp
        let created_at = chrono::Utc::now().to_rfc3339();

        // Store the boost
        let insert_query = r#"
            INSERT OR IGNORE INTO interactions (
                id, type, actor_id, actor_username, actor_display_name,
                actor_avatar_url, post_id, created_at
            ) VALUES (?, 'boost', ?, ?, ?, ?, ?, ?)
        "#;

        let statement = db.prepare(insert_query).bind(&[
            activity.id.as_str().into(),
            activity.actor.as_str().into(),
            actor_username.clone().into(),
            actor_display_name.clone().into(),
            actor_avatar_url.clone().into(),
            object_url.into(),
            created_at.as_str().into(),
        ])?;

        statement.run().await?;

        // Create notification
        create_notification(
            db,
            "boost",
            &activity.actor,
            &actor_username,
            &actor_display_name,
            &actor_avatar_url,
            Some(object_url),
            Some(&activity.id),
            None,
        ).await?;

        console_log!("Stored boost from: {}", activity.actor);
    }

    Ok(())
}

async fn handle_accept(db: &D1Database, activity: &Activity) -> Result<()> {
    console_log!("Processing Accept from: {}", activity.actor);

    // The object should be the Follow activity being accepted
    if let Some(object_id) = activity.object.get("id").and_then(|v| v.as_str()) {
        console_log!("Accept for follow activity: {}", object_id);

        // Update following status to approved
        let accepted_at = chrono::Utc::now().to_rfc3339();

        let query = format!(
            "UPDATE following SET status = 'approved', accepted_at = '{}' WHERE id = '{}'",
            accepted_at, object_id
        );

        let statement = db.prepare(&query);
        statement.run().await?;

        console_log!("Follow request accepted: {}", object_id);
    }

    Ok(())
}

async fn handle_reject(db: &D1Database, activity: &Activity) -> Result<()> {
    console_log!("Processing Reject from: {}", activity.actor);

    // The object should be the Follow activity being rejected
    if let Some(object_id) = activity.object.get("id").and_then(|v| v.as_str()) {
        console_log!("Reject for follow activity: {}", object_id);

        // Update following status to rejected
        let query = format!(
            "UPDATE following SET status = 'rejected' WHERE id = '{}'",
            object_id
        );

        let statement = db.prepare(&query);
        statement.run().await?;

        console_log!("Follow request rejected: {}", object_id);
    }

    Ok(())
}

/// Extract username, display name, and avatar from actor URL
async fn extract_actor_info(actor_url: &str) -> (String, String, String) {
    // Extract username from URL (e.g., https://mastodon.social/users/alice -> @alice@mastodon.social)
    let username = if let Some(parts) = actor_url.split('/').collect::<Vec<_>>().iter().rev().take(2).collect::<Vec<_>>().get(0..2) {
        let handle = parts[1];
        let domain = actor_url.split('/').nth(2).unwrap_or("");
        format!("@{}@{}", handle, domain)
    } else {
        actor_url.to_string()
    };

    // For now, return username as display name and empty avatar
    // TODO: Fetch actor profile to get actual display name and avatar
    (username.clone(), username, String::new())
}

/// Create a notification
async fn create_notification(
    db: &D1Database,
    notification_type: &str,
    actor_id: &str,
    actor_username: &str,
    actor_display_name: &str,
    actor_avatar_url: &str,
    post_id: Option<&str>,
    activity_id: Option<&str>,
    content: Option<&str>,
) -> Result<()> {
    use uuid::Uuid;

    let notification_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    let insert_query = r#"
        INSERT INTO notifications (
            id, type, actor_id, actor_username, actor_display_name,
            actor_avatar_url, post_id, activity_id, content, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;

    let statement = db.prepare(insert_query).bind(&[
        notification_id.into(),
        notification_type.into(),
        actor_id.into(),
        actor_username.into(),
        actor_display_name.into(),
        actor_avatar_url.into(),
        post_id.unwrap_or("").into(),
        activity_id.unwrap_or("").into(),
        content.unwrap_or("").into(),
        created_at.into(),
    ])?;

    statement.run().await?;

    console_log!("Created {} notification", notification_type);

    Ok(())
}

/// Moderate content using Cloudflare AI (Llama Guard 3)
/// Returns: (status, score, flags_json, hidden)
async fn moderate_content(content: &str, ctx: &RouteContext<()>) -> Result<(String, f64, String, bool)> {
    // Get AI binding
    let ai = match ctx.env.ai("AI") {
        Ok(ai) => ai,
        Err(e) => {
            console_log!("Warning: AI binding not available: {}", e);
            // Return default values if AI is not available
            return Ok(("approved".to_string(), 0.0, "[]".to_string(), false));
        }
    };

    // Get moderation settings from database
    let db = ctx.env.d1("DB")?;
    let settings_query = "SELECT auto_hide_threshold, auto_reject_threshold, enabled FROM moderation_settings WHERE id = 1";
    let settings_stmt = db.prepare(settings_query);
    let settings_result = settings_stmt.first::<serde_json::Value>(None).await?;

    let (auto_hide_threshold, auto_reject_threshold, enabled) = if let Some(settings) = settings_result {
        let enabled = settings.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let hide_threshold = settings.get("auto_hide_threshold").and_then(|v| v.as_f64()).unwrap_or(0.7);
        let reject_threshold = settings.get("auto_reject_threshold").and_then(|v| v.as_f64()).unwrap_or(0.9);
        (hide_threshold, reject_threshold, enabled)
    } else {
        (0.7, 0.9, true)
    };

    // If moderation is disabled, approve everything
    if !enabled {
        console_log!("Moderation is disabled, auto-approving");
        return Ok(("approved".to_string(), 0.0, "[]".to_string(), false));
    }

    console_log!("Running Llama Guard 3 moderation on content: {}", &content[..std::cmp::min(50, content.len())]);

    // Prepare the AI request for Llama Guard 3
    // Input format: { messages: [{ role: "user", content: "text" }] }
    let input = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": content
            }
        ]
    });

    // Call Llama Guard 3 model
    let result_json: serde_json::Value = match ai.run::<_, serde_json::Value>("@cf/meta/llama-guard-3-8b", input).await {
        Ok(json) => json,
        Err(e) => {
            console_log!("Warning: Llama Guard 3 moderation failed: {}", e);
            // On error, approve by default (fail-open)
            return Ok(("approved".to_string(), 0.0, "[]".to_string(), false));
        }
    };

    console_log!("Llama Guard 3 response: {}", result_json.to_string());

    // Parse Llama Guard 3 response
    // Response format: { "response": "safe" } or { "response": "unsafe\nS1,S3" }
    let response_text = result_json.get("response")
        .and_then(|v| v.as_str())
        .unwrap_or("safe");

    console_log!("Llama Guard 3 classification: {}", response_text);

    // Parse the response
    let is_safe = response_text.trim().to_lowercase().starts_with("safe");

    if is_safe {
        console_log!("Content classified as safe");
        return Ok(("approved".to_string(), 0.0, "[]".to_string(), false));
    }

    // Parse unsafe categories
    // Format: "unsafe\nS1,S3,S10"
    let lines: Vec<&str> = response_text.lines().collect();
    let categories_str = if lines.len() > 1 {
        lines[1]
    } else {
        ""
    };

    let flagged_categories: Vec<String> = categories_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    console_log!("Flagged categories: {:?}", flagged_categories);

    // Calculate severity score based on category types
    // Llama Guard categories:
    // S1: Violent crimes (0.85)
    // S2: Non-violent crimes (0.65)
    // S3: Sex-related crimes (0.85)
    // S4: Child sexual exploitation (0.95) - CRITICAL
    // S5: Defamation (0.75)
    // S6: Specialized advice (0.60)
    // S7: Privacy (0.65)
    // S8: Intellectual property (0.60)
    // S9: Indiscriminate weapons (0.85)
    // S10: Hate (0.90)
    // S11: Suicide & self-harm (0.90)
    // S12: Sexual content (0.70)
    // S13: Elections (0.65)
    // S14: Code interpreter abuse (0.60)

    let mut max_score = 0.0;
    for category in &flagged_categories {
        let score = match category.as_str() {
            "S4" => 0.95,  // Child exploitation - always reject
            "S10" => 0.90, // Hate
            "S11" => 0.90, // Suicide & self-harm
            "S1" => 0.85,  // Violent crimes
            "S3" => 0.85,  // Sex-related crimes
            "S9" => 0.85,  // Weapons
            "S5" => 0.75,  // Defamation
            "S12" => 0.70, // Sexual content
            "S2" => 0.65,  // Non-violent crimes
            "S7" => 0.65,  // Privacy
            "S13" => 0.65, // Elections
            "S6" => 0.60,  // Specialized advice
            "S8" => 0.60,  // IP
            "S14" => 0.60, // Code abuse
            _ => 0.50,     // Unknown category
        };
        if score > max_score {
            max_score = score;
        }
    }

    console_log!("Calculated severity score: {:.2}", max_score);

    // Determine status and hidden flag based on thresholds
    let (status, hidden) = if max_score >= auto_reject_threshold {
        ("rejected".to_string(), true)
    } else if max_score >= auto_hide_threshold {
        ("hidden".to_string(), true)
    } else {
        ("pending".to_string(), false)
    };

    let flags_json = serde_json::to_string(&flagged_categories).unwrap_or_else(|_| "[]".to_string());

    Ok((status, max_score, flags_json, hidden))
}

/// Check if an actor or their domain is blocked
async fn is_blocked(db: &D1Database, actor_url: &str) -> Result<bool> {
    // Extract domain from actor URL
    // actor_url format: https://instance.social/users/username
    let domain = if let Some(start) = actor_url.find("://") {
        let after_protocol = &actor_url[start + 3..];
        if let Some(end) = after_protocol.find('/') {
            &after_protocol[..end]
        } else {
            after_protocol
        }
    } else {
        return Ok(false); // Invalid URL format, don't block
    };

    // Check if actor or domain is blocked
    let query = format!(
        "SELECT id FROM blocks WHERE actor_id = '{}' OR blocked_domain = '{}' LIMIT 1",
        actor_url, domain
    );

    let statement = db.prepare(&query);
    let result = statement.first::<serde_json::Value>(None).await?;

    Ok(result.is_some())
}
