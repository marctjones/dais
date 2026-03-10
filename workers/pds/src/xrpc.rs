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

    let actor_id = format!("did:web:{}", domain);
    let post_id = format!("at://{}/{}/{}", domain, body.collection, rkey);

    let text_escaped = text.replace("'", "''");
    let record_json = serde_json::to_string(&body.record)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("'", "''");

    let query = format!(
        "INSERT INTO posts (id, actor_id, content, visibility, published_at, protocol, atproto_uri, atproto_cid) \
         VALUES ('{}', '{}', '{}', 'public', '{}', 'atproto', '{}', '{}')",
        post_id, actor_id, text_escaped, created_at, uri, cid
    );

    let statement = db.prepare(&query);
    statement.run().await?;

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

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}
