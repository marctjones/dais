use worker::*;
use serde::Deserialize;
use shared::crypto::HttpSignature;

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
    let mut headers = worker::Headers::new();
    headers.set("Accept", "application/activity+json")?;

    let mut request_init = RequestInit::new();
    request_init.with_method(worker::Method::Get);
    request_init.headers = headers;

    let request = worker::Request::new_with_init(actor_url, &request_init)?;
    let mut response = worker::Fetch::Request(request).send().await?;

    if !response.status_code().is_success() {
        return Err(worker::Error::RustError(format!(
            "Failed to fetch actor: HTTP {}",
            response.status_code()
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
        // Note: Full verification would require rebuilding the signing string
        // from the original request headers. For Phase 1, we'll log that we
        // successfully fetched the key.
        console_log!("✓ Public key fetched successfully");
    }

    // Get D1 database
    let db = ctx.env.d1("DB").expect("D1 database binding not found");

    // Handle different activity types
    match activity.activity_type.as_str() {
        "Follow" => handle_follow(&db, &activity).await?,
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

async fn handle_follow(db: &D1Database, activity: &Activity) -> Result<()> {
    console_log!("Processing Follow from: {}", activity.actor);

    // Extract follower's inbox from their actor object
    // For now, we'll store a placeholder inbox URL
    let follower_inbox = format!("{}/inbox", activity.actor);

    // Insert into followers table with 'pending' status
    let query = r#"
        INSERT OR IGNORE INTO followers (
            id, actor_id, follower_actor_id, follower_inbox, status
        ) VALUES (?, ?, ?, ?, 'pending')
    "#;

    let statement = db.prepare(query).bind(&[
        activity.id.as_str().into(),
        "https://social.dais.social/users/marc".into(), // TODO: Get from username
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
