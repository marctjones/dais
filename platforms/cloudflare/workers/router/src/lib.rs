use serde::Serialize;
use serde_json::{Map, Value};
use worker::{event, Context, Env, Headers, Request, Response, Result};

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();
    let host = url.host_str().unwrap_or_default();

    if host == "social.dais.social" && path == "/" {
        let target = url.join("/users/social")?;
        return Response::redirect(target);
    }

    if path.starts_with("/api/dais/owner/") {
        return handle_owner_api(req, env, path).await;
    }

    match path {
        "/__dais-fixtures/activitypub/actor" => fixture_actor_response(&url),
        "/__dais-fixtures/activitypub/outbox" => fixture_outbox_response(&url),
        "/__dais-fixtures/activitypub/posts/public-preview" => fixture_post_response(&url),
        "/health" => Response::ok("OK"),
        _ => Response::error(
            "Rust router migration scaffold: route not migrated yet",
            501,
        ),
    }
}

async fn handle_owner_api(req: Request, env: Env, path: &str) -> Result<Response> {
    if req.method() == worker::Method::Options {
        return api_json(&serde_json::json!({}), 204);
    }

    let owner_path = path
        .strip_prefix("/api/dais/owner")
        .filter(|value| !value.is_empty())
        .unwrap_or("/");
    if let Some(response) = require_owner_bearer(
        &req,
        &env,
        owner_api_required_scopes(req.method(), owner_path),
    )? {
        return Ok(response);
    }

    match (req.method(), owner_path) {
        (worker::Method::Get, "/profile") => api_json(&owner_profile(&env).await?, 200),
        _ => api_json(
            &serde_json::json!({ "error": "Rust router migration scaffold: owner route not migrated yet" }),
            501,
        ),
    }
}

fn owner_api_required_scopes(method: worker::Method, path: &str) -> &'static [&'static str] {
    match method {
        worker::Method::Get => &["read"],
        worker::Method::Delete => &["write"],
        _ if path == "/discovery/actor" => &["read"],
        _ if path == "/followers/status"
            || path == "/following/follow"
            || path == "/following/unfollow" =>
        {
            &["follow"]
        }
        _ if path.starts_with("/moderation/") => &["moderation"],
        _ if path == "/media" || path == "/media/revoke" => &["media"],
        _ => &["write"],
    }
}

async fn owner_profile(env: &Env) -> Result<OwnerProfile> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type,
                   display_name, summary, icon, image, avatar_url, header_url
            FROM actors
            WHERE username = 'social'
            LIMIT 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let username = string_field(row.as_ref(), "username").unwrap_or_else(|| "social".to_string());
    let actor_url = string_field(row.as_ref(), "id")
        .unwrap_or_else(|| "https://social.dais.social/users/social".to_string());
    let actor_type =
        string_field(row.as_ref(), "actor_type").unwrap_or_else(|| "Person".to_string());
    let handle_domain = env
        .var("DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());
    let icon = string_field(row.as_ref(), "icon");
    let image = string_field(row.as_ref(), "image");
    Ok(OwnerProfile {
        id: actor_url.clone(),
        username: username.clone(),
        actor_type,
        display_name: string_field(row.as_ref(), "display_name"),
        summary: string_field(row.as_ref(), "summary"),
        avatar_url: string_field(row.as_ref(), "avatar_url").or_else(|| icon.clone()),
        header_url: string_field(row.as_ref(), "header_url").or_else(|| image.clone()),
        icon,
        image,
        public_handle: format!("@{username}@{handle_domain}"),
        actor_url,
    })
}

fn string_field(row: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    row.and_then(|fields| fields.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn require_owner_bearer(
    req: &Request,
    env: &Env,
    required_scopes: &[&str],
) -> Result<Option<Response>> {
    let tokens = owner_bearer_tokens(env);
    if tokens.is_empty()
        && env
            .var("ENVIRONMENT")
            .map(|value| value.to_string() == "production")
            .unwrap_or(false)
    {
        return Ok(Some(api_json(
            &serde_json::json!({ "error": "OWNER_API_TOKEN is not configured" }),
            503,
        )?));
    }
    let auth = req.headers().get("Authorization")?.unwrap_or_default();
    let provided = auth.strip_prefix("Bearer ").map(str::trim).unwrap_or("");
    let token = tokens.iter().find(|entry| entry.token == provided);
    match token {
        Some(entry) if owner_token_has_scopes(&entry.scopes, required_scopes) => Ok(None),
        Some(_) => Ok(Some(api_json(
            &serde_json::json!({
                "error": "Owner bearer token lacks required scope",
                "required_scopes": required_scopes,
            }),
            403,
        )?)),
        None => Ok(Some(api_json(
            &serde_json::json!({ "error": "Owner bearer token required" }),
            401,
        )?)),
    }
}

fn owner_bearer_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    let configured = env
        .var("OWNER_API_TOKEN")
        .or_else(|_| env.var("DAIS_OWNER_TOKEN"))
        .map(|value| value.to_string())
        .unwrap_or_else(|_| {
            if env
                .var("ENVIRONMENT")
                .map(|value| value.to_string() == "production")
                .unwrap_or(false)
            {
                String::new()
            } else {
                "dais-local-owner-token".to_string()
            }
        });
    if !configured.is_empty() {
        tokens.push(OwnerToken {
            token: configured,
            scopes: vec!["owner".to_string()],
        });
    }
    tokens.extend(scoped_owner_tokens(env));
    tokens
}

fn scoped_owner_tokens(env: &Env) -> Vec<OwnerToken> {
    let raw = env
        .var("OWNER_API_SCOPED_TOKENS")
        .or_else(|_| env.var("DAIS_OWNER_SCOPED_TOKENS"))
        .map(|value| value.to_string())
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(token, scopes)| {
                let scopes = normalize_scopes(scopes);
                if token.trim().is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        Ok(Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| {
                let token = value
                    .get("token")
                    .or_else(|| value.get("value"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let scopes = normalize_scopes(
                    value
                        .get("scopes")
                        .or_else(|| value.get("scope"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                if token.is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scopes(value: Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(normalize_scope))
            .filter(|scope| !scope.is_empty())
            .collect(),
        Value::String(scopes) => scopes
            .split(|character: char| character == ',' || character.is_whitespace())
            .map(normalize_scope)
            .filter(|scope| !scope.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn owner_token_has_scopes(scopes: &[String], required_scopes: &[&str]) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "owner" || scope == "admin" || scope == "*")
        || required_scopes
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}

fn api_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
    let mut response = Response::from_json(value)?.with_status(status);
    let headers = Headers::new();
    headers.set("Content-Type", "application/json; charset=utf-8")?;
    response = response.with_headers(headers);
    Ok(response)
}

fn fixture_actor_response(url: &worker::Url) -> Result<Response> {
    let public_key = match fixture_public_key(url) {
        Some(value) => value,
        None => return Response::error("Missing or invalid fixture public key", 400),
    };
    let actor_url = url.to_string();
    let name = url
        .query_pairs()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "dais-s2s-fixture".to_string());
    activity_json(&FixtureActor {
        context: "https://www.w3.org/ns/activitystreams",
        id: &actor_url,
        actor_type: "Application",
        preferred_username: &name,
        name: &name,
        inbox: &format!(
            "{}://{}/__dais-fixtures/activitypub/inbox",
            url.scheme(),
            url.host_str().unwrap_or_default()
        ),
        outbox: &fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/outbox"),
        public_key: FixturePublicKey {
            id: &format!("{actor_url}#main-key"),
            owner: &actor_url,
            public_key_pem: &public_key,
        },
    })
}

fn fixture_outbox_response(url: &worker::Url) -> Result<Response> {
    let post = fixture_post(url);
    let create_id = format!("{}#create", post.id);
    activity_json(&FixtureOutbox {
        context: "https://www.w3.org/ns/activitystreams",
        id: &url.to_string(),
        collection_type: "OrderedCollection",
        total_items: 1,
        ordered_items: vec![FixtureCreate {
            id: &create_id,
            create_type: "Create",
            actor: post.attributed_to.clone(),
            to: post.to.clone(),
            object: post,
        }],
    })
}

fn fixture_post_response(url: &worker::Url) -> Result<Response> {
    activity_json(&fixture_post(url))
}

fn fixture_post(url: &worker::Url) -> FixturePost {
    let post_id =
        fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/posts/public-preview");
    FixturePost {
        context: "https://www.w3.org/ns/activitystreams",
        id: post_id.clone(),
        post_type: "Note",
        attributed_to: fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/actor"),
        to: vec![PUBLIC_COLLECTION.to_string()],
        content: "<p>Dais fixture public preview post</p>",
        published: "2026-06-16T00:00:00Z",
        url: post_id,
    }
}

fn activity_json<T: Serialize>(value: &T) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::from_json(value)?.with_headers(headers))
}

fn fixture_public_key(url: &worker::Url) -> Option<String> {
    let encoded = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())?;
    if encoded.len() > 2000
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    let base64 = encoded.replace('-', "+").replace('_', "/");
    let decoded = base64_decode(&base64)?;
    let pem = String::from_utf8(decoded).ok()?;
    if pem.contains("-----BEGIN PUBLIC KEY-----") && pem.contains("-----END PUBLIC KEY-----") {
        Some(pem)
    } else {
        None
    }
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for byte in value.bytes().filter(|byte| *byte != b'=') {
        let index = TABLE.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 6) | index;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Some(output)
}

fn fixture_url_with_public_key(url: &worker::Url, path: &str) -> String {
    let mut next = url.join(path).unwrap_or_else(|_| {
        worker::Url::parse(&format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            path
        ))
        .expect("fixture URL")
    });
    if let Some(public_key) = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())
    {
        next.query_pairs_mut().append_pair("pk", &public_key);
    }
    next.to_string()
}

#[derive(Serialize)]
struct FixtureActor<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    actor_type: &'a str,
    #[serde(rename = "preferredUsername")]
    preferred_username: &'a str,
    name: &'a str,
    inbox: &'a str,
    outbox: &'a str,
    #[serde(rename = "publicKey")]
    public_key: FixturePublicKey<'a>,
}

#[derive(Serialize)]
struct FixturePublicKey<'a> {
    id: &'a str,
    owner: &'a str,
    #[serde(rename = "publicKeyPem")]
    public_key_pem: &'a str,
}

#[derive(Clone, Serialize)]
struct FixturePost {
    #[serde(rename = "@context")]
    context: &'static str,
    id: String,
    #[serde(rename = "type")]
    post_type: &'static str,
    #[serde(rename = "attributedTo")]
    attributed_to: String,
    to: Vec<String>,
    content: &'static str,
    published: &'static str,
    url: String,
}

#[derive(Serialize)]
struct FixtureCreate<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    create_type: &'a str,
    actor: String,
    to: Vec<String>,
    object: FixturePost,
}

#[derive(Serialize)]
struct FixtureOutbox<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    collection_type: &'a str,
    #[serde(rename = "totalItems")]
    total_items: u8,
    #[serde(rename = "orderedItems")]
    ordered_items: Vec<FixtureCreate<'a>>,
}

#[derive(Serialize)]
struct OwnerProfile {
    id: String,
    username: String,
    actor_type: String,
    display_name: Option<String>,
    summary: Option<String>,
    icon: Option<String>,
    image: Option<String>,
    avatar_url: Option<String>,
    header_url: Option<String>,
    public_handle: String,
    actor_url: String,
}

struct OwnerToken {
    token: String,
    scopes: Vec<String>,
}
