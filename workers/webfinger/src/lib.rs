use worker::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct WebFingerQuery {
    resource: String,
}

#[derive(Debug, Serialize)]
struct WebFingerResponse {
    subject: String,
    aliases: Vec<String>,
    links: Vec<WebFingerLink>,
}

#[derive(Debug, Serialize)]
struct WebFingerLink {
    rel: String,
    #[serde(rename = "type")]
    link_type: String,
    href: String,
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/.well-known/webfinger", handle_webfinger)
        .run(req, env)
        .await
}

async fn handle_webfinger(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Parse query parameters
    let url = req.url()?;
    let resource = url
        .query_pairs()
        .find(|(key, _)| key == "resource")
        .map(|(_, value)| value.to_string());

    let resource = match resource {
        Some(r) => r,
        None => {
            return Response::error("Missing 'resource' query parameter", 400);
        }
    };

    // Parse the resource identifier
    // Expected format: acct:marc@dais.social or acct:username@dais.social
    if !resource.starts_with("acct:") {
        return Response::error("Invalid resource format. Expected acct:user@domain", 400);
    }

    let account = resource.strip_prefix("acct:").unwrap();
    let parts: Vec<&str> = account.split('@').collect();

    if parts.len() != 2 {
        return Response::error("Invalid account format. Expected user@domain", 400);
    }

    let username = parts[0];
    let domain = parts[1];

    // Get configured domain from environment variable
    let configured_domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    // Get ActivityPub domain from environment
    let activitypub_domain = ctx.env.var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", configured_domain));

    // Validate domain matches either our base domain or ActivityPub subdomain
    if domain != configured_domain && domain != activitypub_domain {
        return Response::error("Domain not found", 404);
    }

    // Query D1 database to verify user exists
    let db = ctx.env.d1("DB")?;
    let query = "SELECT username FROM actors WHERE username = ?";
    let statement = db.prepare(query).bind(&[username.into()])?;
    let result = statement.first::<serde_json::Value>(None).await?;

    if result.is_none() {
        return Response::error("User not found", 404);
    }

    // Build WebFinger response
    let response = WebFingerResponse {
        subject: resource.clone(),
        aliases: vec![
            format!("https://{}/users/{}", activitypub_domain, username),
        ],
        links: vec![
            WebFingerLink {
                rel: "self".to_string(),
                link_type: "application/activity+json".to_string(),
                href: format!("https://{}/users/{}", activitypub_domain, username),
            },
            WebFingerLink {
                rel: "http://webfinger.net/rel/profile-page".to_string(),
                link_type: "text/html".to_string(),
                href: format!("https://{}/@{}", configured_domain, username),
            },
        ],
    };

    let mut resp = Response::from_json(&response)?;
    resp.headers_mut().set("Content-Type", "application/jrd+json")?;
    resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
    resp.headers_mut().set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    resp.headers_mut().set("Access-Control-Allow-Headers", "Content-Type")?;
    Ok(resp)
}
