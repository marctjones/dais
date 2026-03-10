use worker::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use chrono::Utc;
use crate::auth::{authenticate, validate_token, extract_token};

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    identifier: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct CreateRecordRequest {
    repo: String,
    collection: String,
    record: serde_json::Value,
}

/// Handle com.atproto.server.createSession
pub async fn create_session(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: CreateSessionRequest = req.json().await?;

    match authenticate(&body.identifier, &body.password, &ctx.env) {
        Ok(session) => Response::from_json(&session),
        Err(_) => {
            let error = json!({
                "error": "AuthenticationRequired",
                "message": "Invalid credentials"
            });
            Response::from_json(&error).map(|r| r.with_status(401))
        }
    }
}

/// Handle com.atproto.server.getSession
pub async fn get_session(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => {
            let error = json!({
                "error": "AuthenticationRequired",
                "message": "Missing authorization token"
            });
            return Response::from_json(&error).map(|r| r.with_status(401));
        }
    };

    match validate_token(&token, &ctx.env) {
        Ok(did) => {
            let domain = ctx.env.var("DOMAIN")
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "social.dais.social".to_string());

            let session = json!({
                "did": did,
                "handle": domain,
            });
            Response::from_json(&session)
        }
        Err(_) => {
            let error = json!({
                "error": "InvalidToken",
                "message": "Invalid or expired token"
            });
            Response::from_json(&error).map(|r| r.with_status(401))
        }
    }
}

/// Handle com.atproto.repo.createRecord
pub async fn create_record(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Verify authentication
    let token = match extract_token(&req) {
        Some(t) => t,
        None => {
            let error = json!({"error": "AuthenticationRequired"});
            return Response::from_json(&error).map(|r| r.with_status(401));
        }
    };

    let _did = match validate_token(&token, &ctx.env) {
        Ok(d) => d,
        Err(_) => {
            let error = json!({"error": "InvalidToken"});
            return Response::from_json(&error).map(|r| r.with_status(401));
        }
    };

    let body: CreateRecordRequest = req.json().await?;

    // Only support app.bsky.feed.post for now
    if body.collection != "app.bsky.feed.post" {
        let error = json!({
            "error": "InvalidRequest",
            "message": "Only app.bsky.feed.post is supported"
        });
        return Response::from_json(&error).map(|r| r.with_status(400));
    }

    let db = ctx.env.d1("DB")?;
    let now = Utc::now();
    let rkey = now.timestamp_millis().to_string();
    let uri = format!("at://{}/{}/{}", body.repo, body.collection, rkey);
    let cid = generate_cid(&body.record);

    // Extract text from record
    let text = body.record.get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Store in posts table
    let default_created_at = now.to_rfc3339();
    let created_at = body.record.get("createdAt")
        .and_then(|v| v.as_str())
        .unwrap_or(&default_created_at);

    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    // Use ActivityPub actor ID for database consistency (one identity, two protocols)
    let actor_id = format!("https://social.{}/users/social", domain.split('.').skip(1).collect::<Vec<_>>().join("."));
    let post_id = format!("at://{}/{}/{}", domain, body.collection, rkey);

    let text_escaped = text.replace("'", "''");

    // Check if a post with this content already exists (created by CLI)
    let check_query = format!(
        "SELECT id FROM posts WHERE actor_id = '{}' AND content = '{}' ORDER BY published_at DESC LIMIT 1",
        actor_id, text_escaped
    );

    let check_statement = db.prepare(&check_query);
    let result = check_statement.first::<serde_json::Value>(None).await?;

    // If post exists, update it with AT Protocol metadata; otherwise insert new post
    let query = if let Some(existing) = result {
        let existing_id = existing.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        format!(
            "UPDATE posts SET atproto_uri = '{}', atproto_cid = '{}', protocol = CASE WHEN protocol = 'activitypub' THEN 'both' ELSE protocol END WHERE id = '{}'",
            uri, cid, existing_id
        )
    } else {
        format!(
            "INSERT INTO posts (id, actor_id, content, visibility, published_at, protocol, atproto_uri, atproto_cid) \
             VALUES ('{}', '{}', '{}', 'public', '{}', 'atproto', '{}', '{}')",
            post_id, actor_id, text_escaped, created_at, uri, cid
        )
    };

    let statement = db.prepare(&query);
    statement.run().await?;

    // Broadcast event to relay subscribers
    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());
    let repo_did = format!("did:web:{}", domain);

    let event = crate::relay_subscription::RepoEvent {
        repo: repo_did,
        operation: "create".to_string(),
        path: format!("{}/{}", body.collection, rkey),
        cid: cid.clone(),
        record: body.record.clone(),
    };

    // Get Durable Object and broadcast
    if let Ok(namespace) = ctx.durable_object("RELAY_SUBSCRIPTION") {
        if let Ok(id) = namespace.id_from_name("global") {
            if let Ok(stub) = id.get_stub() {
                let event_json = serde_json::to_string(&event).unwrap_or_default();
                let broadcast_req = Request::new_with_init(
                    "https://internal/broadcast",
                    RequestInit::new()
                        .with_method(Method::Post)
                        .with_body(Some(event_json.into()))
                )?;
                // Fire and forget - don't wait for broadcast to complete
                let _ = stub.fetch_with_request(broadcast_req).await;
            }
        }
    }

    let response = json!({
        "uri": uri,
        "cid": cid,
    });

    Response::from_json(&response)
}

/// Handle com.atproto.repo.listRecords
pub async fn list_records(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let collection = url.query_pairs()
        .find(|(k, _)| k == "collection")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();

    if collection != "app.bsky.feed.post" {
        let error = json!({"error": "InvalidRequest"});
        return Response::from_json(&error).map(|r| r.with_status(400));
    }

    let db = ctx.env.d1("DB")?;
    let query = "SELECT id, content, published_at, atproto_uri, atproto_cid FROM posts WHERE protocol = 'atproto' OR protocol = 'both' ORDER BY published_at DESC LIMIT 50";

    let statement = db.prepare(query);
    let result = statement.all().await?;
    let results_json = result.results::<serde_json::Value>()?;

    let records: Vec<serde_json::Value> = results_json.iter().map(|row| {
        json!({
            "uri": row.get("atproto_uri").and_then(|v| v.as_str()).unwrap_or(""),
            "cid": row.get("atproto_cid").and_then(|v| v.as_str()).unwrap_or(""),
            "value": {
                "text": row.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                "createdAt": row.get("published_at").and_then(|v| v.as_str()).unwrap_or(""),
                "$type": "app.bsky.feed.post"
            }
        })
    }).collect();

    let response = json!({
        "records": records,
    });

    Response::from_json(&response)
}

/// Handle com.atproto.sync.getRepo
pub async fn get_repo(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Return minimal repo data for relay sync
    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let did = format!("did:web:{}", domain);

    // In production, this would return CAR file format
    // For now, return JSON representation
    let response = json!({
        "did": did,
        "head": "bafyreib2rxk3rybk6z5z5z5z5z5z5z5z5z5z5z5z5z5z5z5",
        "blocks": []
    });

    Response::from_json(&response)
}

/// Handle com.atproto.server.describeServer
pub async fn describe_server(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let response = json!({
        "did": format!("did:web:{}", domain),
        "availableUserDomains": [domain],
        "inviteCodeRequired": false,
    });

    Response::from_json(&response)
}

/// Generate a CID for a record (simplified)
fn generate_cid(record: &serde_json::Value) -> String {
    use sha2::{Sha256, Digest};

    let record_str = serde_json::to_string(record).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(record_str.as_bytes());
    let result = hasher.finalize();

    format!("bafyrei{}", hex::encode(&result[..16]))
}

/// Handle com.atproto.sync.listRepos
pub async fn list_repos(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let db = ctx.env.d1("DB")?;
    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let did = format!("did:web:{}", domain);

    // Get latest post to determine revision
    let query = "SELECT COUNT(*) as count FROM posts WHERE protocol IN ('atproto', 'both')";
    let statement = db.prepare(query);
    let result = statement.first::<serde_json::Value>(None).await?;

    let rev = result
        .and_then(|r| r.get("count").and_then(|c| c.as_i64()))
        .unwrap_or(0);

    let repos = vec![json!({
        "did": did,
        "head": format!("bafyreib2rxk3rybk6z5z5z5z5z5z5z5z5z5z5z5z5z5z5z5"),
        "rev": format!("{}", rev),
        "active": true
    })];

    let response = json!({
        "repos": repos,
        "cursor": null
    });

    Response::from_json(&response)
}

/// Handle com.atproto.sync.getRepoStatus
pub async fn get_repo_status(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let did = url.query_pairs()
        .find(|(k, _)| k == "did")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();

    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let expected_did = format!("did:web:{}", domain);

    if did != expected_did {
        let error = json!({"error": "RepoNotFound", "message": "Repository not found"});
        return Response::from_json(&error).map(|r| r.with_status(404));
    }

    let db = ctx.env.d1("DB")?;

    // Get latest post count as revision
    let query = "SELECT COUNT(*) as count FROM posts WHERE protocol IN ('atproto', 'both')";
    let statement = db.prepare(query);
    let result = statement.first::<serde_json::Value>(None).await?;

    let rev = result
        .and_then(|r| r.get("count").and_then(|c| c.as_i64()))
        .unwrap_or(0);

    let response = json!({
        "did": did,
        "active": true,
        "status": "active",
        "rev": format!("{}", rev)
    });

    Response::from_json(&response)
}

/// Handle com.atproto.sync.subscribeRepos (WebSocket endpoint)
pub async fn subscribe_repos(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get the Durable Object stub
    let namespace = ctx.durable_object("RELAY_SUBSCRIPTION")?;
    let stub = namespace.id_from_name("global")?.get_stub()?;

    // Forward the request to the Durable Object
    stub.fetch_with_request(req).await
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}
