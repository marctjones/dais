use dais_cloudflare::D1Provider;
use dais_core::{CoreConfig, DaisCore};
use serde_json::Value;
use shared::theme::Theme;
/// Refactored Actor worker using dais-core
///
/// This is a thin shim that:
/// 1. Extracts platform providers from Cloudflare environment
/// 2. Initializes DaisCore with configuration
/// 3. Calls core.get_actor() / core.get_followers() / core.get_following()
/// 4. Handles content negotiation (JSON vs HTML)
/// 5. Renders HTML using Theme (platform-specific for now)
use worker::{
    self, event, Context, Env, Headers, Method, Request, Response, Result, RouteContext, Router,
};

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/users/:username", handle_actor)
        .get_async("/users/:username/followers", handle_followers)
        .get_async("/users/:username/following", handle_following)
        .get_async("/messages/:message_id", handle_encrypted_message)
        .run(req, env)
        .await
}

async fn handle_encrypted_message(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let message_id = match ctx.param("message_id") {
        Some(id) => id,
        None => return Response::error("Message ID required", 400),
    };

    let db = ctx.env.d1("DB")?;
    let post_pattern = format!("%/posts/{}", message_id);
    let post_rows = db
        .prepare(
            r#"
            SELECT id, content, encrypted_message
            FROM posts
            WHERE encrypted_message IS NOT NULL
              AND (id = ?1 OR id LIKE ?2)
            LIMIT 1
            "#,
        )
        .bind(&[message_id.into(), post_pattern.into()])?
        .all()
        .await?
        .results::<serde_json::Map<String, Value>>()?;

    let row = if let Some(row) = post_rows.into_iter().next() {
        row
    } else {
        let object_pattern = format!("%{}", message_id);
        let timeline_rows = db
            .prepare(
                r#"
                SELECT object_id AS id, content, encrypted_message
                FROM timeline_posts
                WHERE encrypted_message IS NOT NULL
                  AND (object_id = ?1 OR object_id LIKE ?2)
                LIMIT 1
                "#,
            )
            .bind(&[message_id.into(), object_pattern.into()])?
            .all()
            .await?
            .results::<serde_json::Map<String, Value>>()?;

        match timeline_rows.into_iter().next() {
            Some(row) => row,
            None => return Response::error("Encrypted message not found", 404),
        }
    };

    let id = row
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or(message_id);
    let fallback = row
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let encrypted_message = row
        .get("encrypted_message")
        .and_then(|value| value.as_str())
        .unwrap_or("{}");

    let mut headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    add_cors_headers(&mut headers)?;

    Ok(Response::from_html(render_encrypted_message_html(
        id,
        fallback,
        encrypted_message,
    ))?
    .with_headers(headers))
}

async fn handle_actor(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        return handle_cors_preflight();
    }

    let force_json = req
        .url()
        .ok()
        .map(|url| {
            url.query_pairs()
                .any(|(key, value)| key == "format" && value == "json")
        })
        .unwrap_or(false);

    // Check Accept header for content negotiation
    let accept_header = req.headers().get("Accept")?.unwrap_or_default();
    let wants_html = !force_json
        && accept_header.contains("text/html")
        && !accept_header.contains("application/activity+json");

    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

    // Get configuration from environment variables
    let activitypub_domain = ctx
        .env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let config = CoreConfig {
        activitypub_domain: activitypub_domain.clone(),
        pds_domain: "".to_string(),
        username: ctx
            .env
            .var("USERNAME")
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "social".to_string()),
        private_key: "".to_string(),
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    // Initialize DaisCore (with placeholder providers for unused features)
    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(PlaceholderHttp),
        config,
    );

    // Call core logic - get actor
    let person = match core.get_actor(username.to_string()).await {
        Ok(p) => p,
        Err(e) => {
            return match e {
                dais_core::CoreError::NotFound(msg) => Response::error(msg, 404),
                _ => Response::error(format!("Internal error: {}", e), 500),
            };
        }
    };

    // Get actor ID from person.id (format: https://social.dais.social/users/username)
    let actor_id = person.id.clone();

    // Get counts for HTML rendering
    let counts = if wants_html {
        core.get_actor_counts(actor_id).await.ok()
    } else {
        None
    };

    // Build response based on content negotiation
    if wants_html {
        let mut headers = Headers::new();
        headers.set("Content-Type", "text/html; charset=utf-8")?;
        add_cors_headers(&mut headers)?;

        let theme_name = ctx
            .env
            .var("THEME")
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "dais".to_string());
        let theme = Theme::from_name(&theme_name);

        let post_count = counts.as_ref().map(|c| c.post_count as usize).unwrap_or(0);
        let follower_count = counts
            .as_ref()
            .map(|c| c.follower_count as usize)
            .unwrap_or(0);

        let icon_url = person.icon.as_ref().map(|i| i.url.clone());
        let image_url = person.image.as_ref().map(|i| i.url.clone());

        let html = render_profile_html(
            &person,
            &username,
            post_count,
            follower_count,
            &theme,
            icon_url,
            image_url,
        );
        Ok(Response::from_html(html)?.with_headers(headers))
    } else {
        let mut headers = Headers::new();
        headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
        add_cors_headers(&mut headers)?;
        Ok(Response::from_json(&person)?.with_headers(headers))
    }
}

async fn handle_followers(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        return handle_cors_preflight();
    }

    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    // Check for page parameter
    let url = req.url()?;
    let page: Option<u32> = url
        .query_pairs()
        .find(|(k, _)| k == "page")
        .and_then(|(_, v)| v.parse().ok());

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

    let activitypub_domain = ctx
        .env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let config = CoreConfig {
        activitypub_domain,
        pds_domain: "".to_string(),
        username: "".to_string(),
        private_key: "".to_string(),
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(PlaceholderHttp),
        config,
    );

    // Get followers collection from core
    let collection = match core.get_followers(username.to_string(), page).await {
        Ok(c) => c,
        Err(e) => return Response::error(format!("Error: {}", e), 500),
    };

    let mut headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    add_cors_headers(&mut headers)?;
    Ok(Response::from_json(&collection)?.with_headers(headers))
}

async fn handle_following(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Handle OPTIONS request
    if req.method() == Method::Options {
        return handle_cors_preflight();
    }

    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    // Check for page parameter
    let url = req.url()?;
    let page: Option<u32> = url
        .query_pairs()
        .find(|(k, _)| k == "page")
        .and_then(|(_, v)| v.parse().ok());

    // Initialize platform providers
    let db = D1Provider::new(ctx.env.d1("DB")?);

    let activitypub_domain = ctx
        .env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let config = CoreConfig {
        activitypub_domain,
        pds_domain: "".to_string(),
        username: "".to_string(),
        private_key: "".to_string(),
        public_key: "".to_string(),
        media_url: "".to_string(),
    };

    let core = DaisCore::new(
        Box::new(db),
        Box::new(PlaceholderStorage),
        Box::new(PlaceholderQueue),
        Box::new(PlaceholderHttp),
        config,
    );

    // Get following collection from core
    let collection = match core.get_following(username.to_string(), page).await {
        Ok(c) => c,
        Err(e) => return Response::error(format!("Error: {}", e), 500),
    };

    let mut headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    add_cors_headers(&mut headers)?;
    Ok(Response::from_json(&collection)?.with_headers(headers))
}

// CORS helpers

fn handle_cors_preflight() -> Result<Response> {
    let headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;
    Ok(Response::empty()?.with_headers(headers))
}

fn add_cors_headers(headers: &mut Headers) -> Result<()> {
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;
    Ok(())
}

// HTML rendering (platform-specific for now - uses Cloudflare Workers Theme)

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_encrypted_message_html(id: &str, fallback: &str, encrypted_message: &str) -> String {
    let escaped_id = escape_html(id);
    let escaped_fallback = escape_html(fallback);
    let encrypted_json = serde_json::from_str::<Value>(encrypted_message)
        .unwrap_or(Value::Null)
        .to_string();

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Encrypted dais message</title>
  <style>
    body {{ font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 0; background: #f7f7f2; color: #202124; }}
    main {{ max-width: 760px; margin: 0 auto; padding: 32px 20px; }}
    h1 {{ font-size: 28px; margin: 0 0 16px; }}
    .panel {{ background: #fff; border: 1px solid #ddd8cc; border-radius: 8px; padding: 20px; margin: 16px 0; }}
    label {{ display: block; font-weight: 650; margin-bottom: 8px; }}
    textarea, input {{ width: 100%; box-sizing: border-box; font: 14px ui-monospace, SFMono-Regular, Menlo, monospace; border: 1px solid #b8b1a2; border-radius: 6px; padding: 10px; }}
    textarea {{ min-height: 160px; }}
    button {{ appearance: none; border: 0; border-radius: 6px; background: #205c4a; color: white; padding: 10px 14px; font-weight: 700; cursor: pointer; }}
    button + button {{ margin-left: 8px; }}
    pre {{ white-space: pre-wrap; overflow-wrap: anywhere; }}
    .muted {{ color: #5f6368; }}
    .error {{ color: #9b1c1c; }}
  </style>
</head>
<body>
<main>
  <h1>Encrypted dais message</h1>
  <p class="muted">Message id: {escaped_id}</p>
  <div class="panel">
    <strong>Fallback shown to non-dais clients</strong>
    <pre>{escaped_fallback}</pre>
  </div>
  <div class="panel">
    <label for="private-key">Private key PEM</label>
    <textarea id="private-key" autocomplete="off" spellcheck="false" placeholder="Paste a PKCS#8 RSA private key. The key stays in this browser and is not sent to dais."></textarea>
    <p class="muted">If the URL has <code>#cek=...</code>, this page can decrypt with that link key instead. Do not put link keys in federation fallback content if you need confidentiality from the recipient server.</p>
    <button id="decrypt">Decrypt</button>
  </div>
  <div class="panel">
    <strong>Plaintext</strong>
    <pre id="plaintext" class="muted">Not decrypted yet.</pre>
  </div>
  <details class="panel">
    <summary>encryptedMessage JSON</summary>
    <pre id="encrypted-json"></pre>
  </details>
</main>
<script>
const encryptedMessage = {encrypted_json};
document.getElementById("encrypted-json").textContent = JSON.stringify(encryptedMessage, null, 2);

const dec = new TextDecoder();
function b64bytes(value) {{
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}}
function pemBytes(pem) {{
  const body = pem.replace(/-----BEGIN PRIVATE KEY-----/g, "")
    .replace(/-----END PRIVATE KEY-----/g, "")
    .replace(/\s+/g, "");
  return b64bytes(body);
}}
function fragmentParam(name) {{
  const hash = window.location.hash.startsWith("#") ? window.location.hash.slice(1) : window.location.hash;
  return new URLSearchParams(hash).get(name);
}}
async function aesDecrypt(cekBytes) {{
  const key = await crypto.subtle.importKey("raw", cekBytes, "AES-GCM", false, ["decrypt"]);
  const plaintext = await crypto.subtle.decrypt(
    {{ name: "AES-GCM", iv: b64bytes(encryptedMessage.iv) }},
    key,
    b64bytes(encryptedMessage.ciphertext)
  );
  return dec.decode(plaintext);
}}
async function decryptWithPrivateKey(pem) {{
  const recipient = encryptedMessage.recipients && encryptedMessage.recipients.length === 1
    ? encryptedMessage.recipients[0]
    : null;
  if (!recipient) throw new Error("Paste-key decrypt currently requires a single-recipient envelope.");
  const key = await crypto.subtle.importKey(
    "pkcs8",
    pemBytes(pem),
    {{ name: "RSA-OAEP", hash: "SHA-256" }},
    false,
    ["decrypt"]
  );
  const cek = await crypto.subtle.decrypt(
    {{ name: "RSA-OAEP" }},
    key,
    b64bytes(recipient.wrappedKey)
  );
  return aesDecrypt(new Uint8Array(cek));
}}
document.getElementById("decrypt").addEventListener("click", async () => {{
  const output = document.getElementById("plaintext");
  output.className = "muted";
  output.textContent = "Decrypting...";
  try {{
    const cek = fragmentParam("cek");
    const plaintext = cek
      ? await aesDecrypt(b64bytes(cek))
      : await decryptWithPrivateKey(document.getElementById("private-key").value);
    output.className = "";
    output.textContent = plaintext;
  }} catch (error) {{
    output.className = "error";
    output.textContent = error && error.message ? error.message : String(error);
  }}
}});
</script>
</body>
</html>"##
    )
}

fn render_profile_html(
    person: &dais_core::activitypub::Person,
    username: &str,
    post_count: usize,
    follower_count: usize,
    theme: &Theme,
    icon_url: Option<String>,
    image_url: Option<String>,
) -> String {
    let display_name = person.name.as_ref().unwrap_or(&person.preferred_username);
    let summary = person.summary.as_deref().unwrap_or("");
    let actor_domain = actor_domain_from_id(&person.id).unwrap_or("social.dais.social");

    let light = &theme.light;
    let dark = &theme.dark;

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{display_name} (@{username}@{actor_domain})</title>
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
        .profile-card {{
            background: {bg_secondary};
            border-radius: 16px;
            padding: 48px;
            margin-bottom: 20px;
        }}
        .header {{
            text-align: center;
            margin-bottom: 32px;
        }}
        .profile-header {{
            position: relative;
            margin: -48px -48px 32px;
            border-radius: 16px 16px 0 0;
            overflow: hidden;
        }}
        .header-image {{
            width: 100%;
            height: 200px;
            object-fit: cover;
            display: block;
        }}
        .header-placeholder {{
            width: 100%;
            height: 200px;
            background: linear-gradient(135deg, {accent_primary} 0%, {accent_hover} 100%);
        }}
        .avatar {{
            width: 120px;
            height: 120px;
            border-radius: 50%;
            margin: 0 auto 20px;
            position: relative;
            border: 4px solid {bg_secondary};
        }}
        .avatar-image {{
            width: 100%;
            height: 100%;
            border-radius: 50%;
            object-fit: cover;
            display: block;
        }}
        .avatar-letter {{
            width: 100%;
            height: 100%;
            background: linear-gradient(135deg, {accent_primary} 0%, {accent_hover} 100%);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 48px;
            color: white;
            font-weight: 700;
        }}
        h1 {{
            font-size: 32px;
            margin-bottom: 8px;
            color: {text_primary};
            font-weight: 700;
        }}
        .handle {{
            color: {text_secondary};
            font-size: 16px;
        }}
        .bio {{
            margin: 32px 0;
            padding: 24px;
            background: {bg_primary};
            border-radius: 12px;
            color: {text_primary};
            border-left: 4px solid {accent_primary};
        }}
        .stats {{
            display: flex;
            justify-content: space-around;
            margin: 32px 0;
            padding: 24px 0;
            border-top: 1px solid {border};
            border-bottom: 1px solid {border};
        }}
        .stat {{
            text-align: center;
        }}
        .stat-value {{
            font-size: 28px;
            font-weight: 700;
            color: {accent_primary};
        }}
        .stat-label {{
            color: {text_secondary};
            font-size: 14px;
            margin-top: 4px;
        }}
        .actions {{
            display: flex;
            gap: 12px;
            justify-content: center;
            margin-top: 32px;
        }}
        .button {{
            padding: 12px 24px;
            border-radius: 8px;
            font-size: 15px;
            font-weight: 600;
            text-decoration: none;
            transition: all 0.2s ease;
        }}
        .button-primary {{
            background: {accent_primary};
            color: white;
        }}
        .button-primary:hover {{
            background: {accent_hover};
        }}
        .button-secondary {{
            background: {bg_primary};
            color: {text_primary};
            border: 2px solid {border};
        }}
        .button-secondary:hover {{
            border-color: {accent_primary};
            color: {accent_hover};
        }}
        .footer {{
            text-align: center;
            margin-top: 32px;
            padding-top: 24px;
            border-top: 1px solid {border};
            color: {text_secondary};
            font-size: 14px;
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
            .profile-card {{
                background: {dark_bg_secondary};
            }}
            .avatar {{
                border-color: {dark_bg_secondary};
            }}
            .header-placeholder {{
                background: linear-gradient(135deg, {dark_accent_primary} 0%, {dark_accent_hover} 100%);
            }}
            .avatar-letter {{
                background: linear-gradient(135deg, {dark_accent_primary} 0%, {dark_accent_hover} 100%);
            }}
            h1 {{
                color: {dark_text_primary};
            }}
            .handle, .stat-label {{
                color: {dark_text_secondary};
            }}
            .bio {{
                background: {dark_bg_primary};
                color: {dark_text_primary};
                border-left-color: {dark_accent_primary};
            }}
            .stat-value {{
                color: {dark_accent_primary};
            }}
            .stats {{
                border-top-color: {dark_border};
                border-bottom-color: {dark_border};
            }}
            .button-primary {{
                background: {dark_accent_primary};
                color: {dark_bg_primary};
            }}
            .button-primary:hover {{
                background: {dark_accent_hover};
            }}
            .button-secondary {{
                background: {dark_bg_primary};
                color: {dark_text_primary};
                border-color: {dark_border};
            }}
            .button-secondary:hover {{
                border-color: {dark_accent_primary};
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
        <div class="profile-card">
            {header_html}
            <div class="header">
                {avatar_html}
                <h1>{display_name}</h1>
                <div class="handle">@{handle_username}@{actor_domain}</div>
            </div>

            {bio_html}

            <div class="stats">
                <div class="stat">
                    <div class="stat-value">{post_count}</div>
                    <div class="stat-label">Posts</div>
                </div>
                <div class="stat">
                    <div class="stat-value">0</div>
                    <div class="stat-label">Following</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{follower_count}</div>
                    <div class="stat-label">Followers</div>
                </div>
            </div>

            <div class="actions">
                <a href="/users/{outbox_username}/outbox" class="button button-primary">View Posts</a>
                <a href="{person_id}?format=json" class="button button-secondary">ActivityPub JSON</a>
            </div>
        </div>

        <div class="footer">
            <p>Powered by <a href="https://dais.social">dais</a> - Self-hosted ActivityPub</p>
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
        // Dark mode colors
        dark_bg_primary = dark.bg_primary,
        dark_bg_secondary = dark.bg_secondary,
        dark_text_primary = dark.text_primary,
        dark_text_secondary = dark.text_secondary,
        dark_accent_primary = dark.accent_primary,
        dark_accent_hover = dark.accent_hover,
        dark_border = dark.border,
        // Content
        header_html = if let Some(ref header_url) = image_url {
            format!(
                r#"<div class="profile-header"><img src="{}" alt="Profile header" class="header-image"></div>"#,
                header_url
            )
        } else {
            String::new()
        },
        avatar_html = if let Some(ref avatar_url) = icon_url {
            format!(
                r#"<div class="avatar"><img src="{}" alt="Profile picture" class="avatar-image"></div>"#,
                avatar_url
            )
        } else {
            format!(
                r#"<div class="avatar"><div class="avatar-letter">{}</div></div>"#,
                display_name.chars().next().unwrap_or('?').to_uppercase()
            )
        },
        display_name = display_name,
        handle_username = username,
        actor_domain = actor_domain,
        bio_html = if !summary.is_empty() {
            format!(r#"<div class="bio">{}</div>"#, summary)
        } else {
            String::new()
        },
        post_count = post_count,
        follower_count = follower_count,
        outbox_username = username,
        person_id = person.id
    )
}

fn actor_domain_from_id(actor_id: &str) -> Option<&str> {
    actor_id
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .filter(|domain| !domain.is_empty())
}

// Placeholder providers for unused platform features

use async_trait::async_trait;
use dais_core::traits::*;

struct PlaceholderStorage;

#[async_trait(?Send)]
impl StorageProvider for PlaceholderStorage {
    async fn put(&self, _key: &str, _data: Vec<u8>, _content_type: &str) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn put_with_metadata(
        &self,
        _key: &str,
        _data: Vec<u8>,
        _content_type: &str,
        _metadata: StorageMetadata,
    ) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn get(&self, _key: &str) -> PlatformResult<Vec<u8>> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn head(&self, _key: &str) -> PlatformResult<ObjectInfo> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn delete(&self, _key: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn list(&self, _prefix: &str) -> PlatformResult<Vec<String>> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn list_detailed(
        &self,
        _options: dais_core::traits::ListOptions,
    ) -> PlatformResult<dais_core::traits::ListResult> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn copy(&self, _from: &str, _to: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    fn public_url(&self, _key: &str) -> String {
        String::new()
    }

    async fn signed_url(&self, _key: &str, _expires_in: u32) -> PlatformResult<String> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }
}

struct PlaceholderQueue;

#[async_trait(?Send)]
impl QueueProvider for PlaceholderQueue {
    async fn send(&self, _message: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn send_batch(&self, _messages: Vec<String>) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn send_delayed(&self, _message: &str, _delay_seconds: u32) -> PlatformResult<()> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }

    async fn depth(&self) -> PlatformResult<u64> {
        Ok(0)
    }
}

struct PlaceholderHttp;

#[async_trait(?Send)]
impl HttpProvider for PlaceholderHttp {
    async fn fetch(
        &self,
        _request: dais_core::traits::Request,
    ) -> PlatformResult<dais_core::traits::Response> {
        Err(PlatformError::Internal("Not implemented".to_string()))
    }
}
