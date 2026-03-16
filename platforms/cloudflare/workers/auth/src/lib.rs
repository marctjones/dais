/// Refactored Auth worker
///
/// This worker handles authentication for the admin user.
/// It provides login/logout and session validation endpoints.
///
/// Note: This is a simple implementation. In production, you would want:
/// - Password hashing (bcrypt, argon2)
/// - Rate limiting
/// - CSRF protection
/// - Secure session management

use worker::*;
use dais_cloudflare::D1Provider;
use dais_core::traits::DatabaseProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    success: bool,
    token: Option<String>,
    message: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    valid: bool,
    username: Option<String>,
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .options("/*", |_req, _ctx| {
            let headers = Headers::new();
            headers.set("Access-Control-Allow-Origin", "*")?;
            headers.set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")?;
            headers.set("Access-Control-Allow-Headers", "Content-Type, Authorization")?;
            headers.set("Access-Control-Max-Age", "86400")?;
            Ok(Response::empty()?.with_headers(headers))
        })
        .post_async("/auth/login", handle_login)
        .post_async("/auth/logout", handle_logout)
        .get_async("/auth/validate", handle_validate)
        .run(req, env)
        .await
}

async fn handle_login(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Parse login request
    let body: LoginRequest = match req.json().await {
        Ok(b) => b,
        Err(_) => {
            return Response::from_json(&LoginResponse {
                success: false,
                token: None,
                message: "Invalid request body".to_string(),
            });
        }
    };

    // Get expected password from environment
    let expected_password = ctx.env.var("ADMIN_PASSWORD")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| {
            console_log!("WARNING: ADMIN_PASSWORD not set");
            String::new()
        });

    // Verify password (in production, use password hashing!)
    if body.password != expected_password {
        return Response::from_json(&LoginResponse {
            success: false,
            token: None,
            message: "Invalid password".to_string(),
        });
    }

    // Generate session token
    let token = uuid::Uuid::new_v4().to_string();

    // Get username from environment
    let username = ctx.env.var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "admin".to_string());

    // Store session in database
    let db = ctx.env.d1("DB")?;
    let db_provider = D1Provider::new(db);

    let now = chrono::Utc::now().to_rfc3339();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let expires_at_str = expires_at.to_rfc3339();

    let query = r#"
        INSERT INTO sessions (token, username, created_at, expires_at)
        VALUES (?1, ?2, ?3, ?4)
    "#;

    match db_provider.execute(query, &[
        Value::String(token.clone()),
        Value::String(username.clone()),
        Value::String(now),
        Value::String(expires_at_str),
    ]).await {
        Ok(_) => {
            Response::from_json(&LoginResponse {
                success: true,
                token: Some(token),
                message: "Login successful".to_string(),
            })
        }
        Err(e) => {
            console_log!("Database error: {}", e);
            Response::from_json(&LoginResponse {
                success: false,
                token: None,
                message: "Internal server error".to_string(),
            })
        }
    }
}

async fn handle_logout(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get token from Authorization header
    let token = match req.headers().get("Authorization")? {
        Some(auth) => auth.trim_start_matches("Bearer ").to_string(),
        None => {
            return Response::error("Missing Authorization header", 401);
        }
    };

    // Delete session from database
    let db = ctx.env.d1("DB")?;
    let db_provider = D1Provider::new(db);

    let query = "DELETE FROM sessions WHERE token = ?1";

    match db_provider.execute(query, &[Value::String(token)]).await {
        Ok(_) => Response::ok("Logged out successfully"),
        Err(e) => {
            console_log!("Database error: {}", e);
            Response::error("Internal server error", 500)
        }
    }
}

async fn handle_validate(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Get token from Authorization header
    let token = match req.headers().get("Authorization")? {
        Some(auth) => auth.trim_start_matches("Bearer ").to_string(),
        None => {
            return Response::from_json(&ValidateResponse {
                valid: false,
                username: None,
            });
        }
    };

    // Check if session exists and is not expired
    let db = ctx.env.d1("DB")?;
    let db_provider = D1Provider::new(db);

    let now = chrono::Utc::now().to_rfc3339();
    let query = r#"
        SELECT username FROM sessions
        WHERE token = ?1 AND expires_at > ?2
    "#;

    match db_provider.execute(query, &[
        Value::String(token),
        Value::String(now),
    ]).await {
        Ok(rows) => {
            if rows.is_empty() {
                Response::from_json(&ValidateResponse {
                    valid: false,
                    username: None,
                })
            } else {
                let username = rows[0].get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("admin")
                    .to_string();

                Response::from_json(&ValidateResponse {
                    valid: true,
                    username: Some(username),
                })
            }
        }
        Err(e) => {
            console_log!("Database error: {}", e);
            Response::from_json(&ValidateResponse {
                valid: false,
                username: None,
            })
        }
    }
}
