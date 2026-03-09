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
            let mut headers = Headers::new();
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

    // Handle different activity types
    match activity.activity_type.as_str() {
        "Follow" => handle_follow(&db, &activity, username).await?,
        "Undo" => handle_undo(&db, &activity).await?,
        "Accept" | "Reject" => {
            console_log!("Received {} activity", activity.activity_type);
            // These are responses to our follow requests (Phase 3)
        }
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
        }
    }

    Ok(())
}
