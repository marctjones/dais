use worker::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::time::{SystemTime, UNIX_EPOCH};

mod utils;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Serialize)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    token_type: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct TokenClaims {
    sub: String,        // subject (username)
    iat: u64,           // issued at
    exp: u64,           // expiration
    token_type: String, // "access" or "refresh"
}

fn hash_password(password: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}{}", salt, password));
    let result = hasher.finalize();
    BASE64.encode(result)
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn create_jwt(username: &str, token_type: &str, expiry_seconds: u64, secret: &str) -> Result<String> {
    let now = current_timestamp();
    let claims = TokenClaims {
        sub: username.to_string(),
        iat: now,
        exp: now + expiry_seconds,
        token_type: token_type.to_string(),
    };

    // Simple JWT implementation (header.payload.signature)
    let header = json!({
        "alg": "HS256",
        "typ": "JWT"
    });

    let header_b64 = BASE64.encode(header.to_string());
    let payload_b64 = BASE64.encode(serde_json::to_string(&claims)?);

    let message = format!("{}.{}", header_b64, payload_b64);

    // HMAC-SHA256 signature
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| Error::RustError(format!("HMAC error: {}", e)))?;
    mac.update(message.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());

    Ok(format!("{}.{}", message, signature))
}

fn verify_jwt(token: &str, secret: &str) -> Result<TokenClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(Error::RustError("Invalid JWT format".into()));
    }

    let message = format!("{}.{}", parts[0], parts[1]);

    // Verify signature
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| Error::RustError(format!("HMAC error: {}", e)))?;
    mac.update(message.as_bytes());
    let expected_sig = BASE64.encode(mac.finalize().into_bytes());

    if expected_sig != parts[2] {
        return Err(Error::RustError("Invalid signature".into()));
    }

    // Decode payload
    let payload = String::from_utf8(BASE64.decode(parts[1])?)
        .map_err(|e| Error::RustError(format!("UTF-8 error: {}", e)))?;

    let claims: TokenClaims = serde_json::from_str(&payload)?;

    // Check expiration
    if claims.exp < current_timestamp() {
        return Err(Error::RustError("Token expired".into()));
    }

    Ok(claims)
}

async fn handle_login(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: LoginRequest = req.json().await?;

    // Get environment variables
    let username = ctx.var("USERNAME")?.to_string();
    let password_hash = ctx.var("PASSWORD_HASH")?.to_string();
    let salt = ctx.var("PASSWORD_SALT")?.to_string();
    let jwt_secret = ctx.var("JWT_SECRET")?.to_string();

    // Verify username
    if body.username != username {
        return Response::from_json(&ErrorResponse {
            error: "invalid_credentials".to_string(),
            message: "Invalid username or password".to_string(),
        })?.with_status(401);
    }

    // Verify password
    let input_hash = hash_password(&body.password, &salt);
    if input_hash != password_hash {
        return Response::from_json(&ErrorResponse {
            error: "invalid_credentials".to_string(),
            message: "Invalid username or password".to_string(),
        })?.with_status(401);
    }

    // Generate tokens
    let access_token = create_jwt(&username, "access", 3600, &jwt_secret)?; // 1 hour
    let refresh_token = create_jwt(&username, "refresh", 2592000, &jwt_secret)?; // 30 days

    // Store refresh token in D1 (for revocation capability)
    let db = ctx.env.d1("DB")?;
    let _result = db.prepare(
        "INSERT INTO auth_tokens (username, refresh_token, created_at, expires_at) VALUES (?, ?, ?, ?)"
    )
    .bind(&[
        username.into(),
        refresh_token.clone().into(),
        current_timestamp().to_string().into(),
        (current_timestamp() + 2592000).to_string().into(),
    ])?
    .run()
    .await?;

    Response::from_json(&AuthResponse {
        access_token,
        refresh_token,
        expires_in: 3600,
        token_type: "Bearer".to_string(),
    })
}

async fn handle_refresh(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: RefreshRequest = req.json().await?;
    let jwt_secret = ctx.var("JWT_SECRET")?.to_string();

    // Verify refresh token
    let claims = match verify_jwt(&body.refresh_token, &jwt_secret) {
        Ok(c) => c,
        Err(_) => {
            return Response::from_json(&ErrorResponse {
                error: "invalid_token".to_string(),
                message: "Invalid or expired refresh token".to_string(),
            })?.with_status(401);
        }
    };

    if claims.token_type != "refresh" {
        return Response::from_json(&ErrorResponse {
            error: "invalid_token".to_string(),
            message: "Not a refresh token".to_string(),
        })?.with_status(401);
    }

    // Check if token exists in database (not revoked)
    let db = ctx.env.d1("DB")?;
    let result = db.prepare(
        "SELECT username FROM auth_tokens WHERE refresh_token = ? AND expires_at > ?"
    )
    .bind(&[
        body.refresh_token.clone().into(),
        current_timestamp().to_string().into(),
    ])?
    .first::<serde_json::Value>(None)
    .await?;

    if result.is_none() {
        return Response::from_json(&ErrorResponse {
            error: "invalid_token".to_string(),
            message: "Refresh token revoked or expired".to_string(),
        })?.with_status(401);
    }

    // Generate new access token
    let access_token = create_jwt(&claims.sub, "access", 3600, &jwt_secret)?;

    Response::from_json(&json!({
        "access_token": access_token,
        "expires_in": 3600,
        "token_type": "Bearer"
    }))
}

async fn handle_logout(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: RefreshRequest = req.json().await?;

    // Revoke refresh token by deleting from database
    let db = ctx.env.d1("DB")?;
    let _result = db.prepare("DELETE FROM auth_tokens WHERE refresh_token = ?")
        .bind(&[body.refresh_token.into()])?
        .run()
        .await?;

    Response::from_json(&json!({
        "message": "Logged out successfully"
    }))
}

async fn handle_verify(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let jwt_secret = ctx.var("JWT_SECRET")?.to_string();

    // Extract token from Authorization header
    let auth_header = req.headers().get("Authorization")?;
    let token = match auth_header {
        Some(h) => {
            if let Some(t) = h.strip_prefix("Bearer ") {
                t
            } else {
                return Response::from_json(&ErrorResponse {
                    error: "invalid_token".to_string(),
                    message: "Invalid authorization header format".to_string(),
                })?.with_status(401);
            }
        }
        None => {
            return Response::from_json(&ErrorResponse {
                error: "missing_token".to_string(),
                message: "Authorization header required".to_string(),
            })?.with_status(401);
        }
    };

    // Verify token
    match verify_jwt(token, &jwt_secret) {
        Ok(claims) => {
            if claims.token_type != "access" {
                return Response::from_json(&ErrorResponse {
                    error: "invalid_token".to_string(),
                    message: "Not an access token".to_string(),
                })?.with_status(401);
            }

            Response::from_json(&json!({
                "valid": true,
                "username": claims.sub,
                "expires_at": claims.exp
            }))
        }
        Err(_) => {
            Response::from_json(&ErrorResponse {
                error: "invalid_token".to_string(),
                message: "Invalid or expired token".to_string(),
            })?.with_status(401)
        }
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    utils::set_panic_hook();

    let router = Router::new();

    router
        .post_async("/api/auth/login", |req, ctx| async move {
            handle_login(req, ctx).await
        })
        .post_async("/api/auth/refresh", |req, ctx| async move {
            handle_refresh(req, ctx).await
        })
        .post_async("/api/auth/logout", |req, ctx| async move {
            handle_logout(req, ctx).await
        })
        .get_async("/api/auth/verify", |req, ctx| async move {
            handle_verify(req, ctx).await
        })
        .run(req, env)
        .await
}
