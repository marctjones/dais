use worker::*;
use serde::{Deserialize, Serialize};
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
}

/// Verify credentials and create session
pub fn authenticate(
    identifier: &str,
    password: &str,
    env: &Env
) -> Result<Session> {
    // Get configured credentials from secrets
    let expected_handle = env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let expected_password = env.secret("PDS_PASSWORD")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "changeme".to_string());

    // Verify credentials
    if identifier != expected_handle || password != expected_password {
        return Err(Error::RustError("Invalid credentials".to_string()));
    }

    let did = format!("did:web:{}", expected_handle);

    // Generate simple JWT (in production, use proper JWT signing)
    let access_jwt = generate_jwt(&did, "access");
    let refresh_jwt = generate_jwt(&did, "refresh");

    Ok(Session {
        did,
        handle: expected_handle,
        access_jwt,
        refresh_jwt,
    })
}

/// Validate an access token
pub fn validate_token(token: &str, env: &Env) -> Result<String> {
    // Extract DID from token (simplified - in production, verify signature)
    if token.starts_with("dais.") {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() >= 3 {
            // Decode the payload (simplified)
            let did = format!("did:web:{}",
                env.var("DOMAIN")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| "social.dais.social".to_string())
            );
            return Ok(did);
        }
    }
    Err(Error::RustError("Invalid token".to_string()))
}

/// Generate a simple JWT (simplified - in production use proper signing)
fn generate_jwt(did: &str, typ: &str) -> String {
    let now = Utc::now().timestamp();
    let exp = now + 3600; // 1 hour

    // Simplified JWT format: header.payload.signature
    format!("dais.{}.{}.{}", typ, did, exp)
}

/// Extract bearer token from Authorization header
pub fn extract_token(req: &Request) -> Option<String> {
    req.headers()
        .get("Authorization")
        .ok()
        .flatten()
        .and_then(|auth| {
            if auth.starts_with("Bearer ") {
                Some(auth[7..].to_string())
            } else {
                None
            }
        })
}
