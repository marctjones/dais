use worker::*;
use shared::activitypub::Person;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::new();

    router
        .get_async("/users/:username", handle_actor)
        .run(req, env)
        .await
}

async fn handle_actor(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Add CORS headers for federation
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "Content-Type, Accept")?;

    // Handle OPTIONS request
    if req.method() == Method::Options {
        return Ok(Response::empty()?.with_headers(headers));
    }

    // Get username from URL
    let username = match ctx.param("username") {
        Some(u) => u,
        None => return Response::error("Username required", 400),
    };

    // Get D1 database
    let db = ctx.env.d1("DB").expect("D1 database binding not found");

    // Query for actor
    let query = "SELECT id, username, display_name, summary, public_key FROM actors WHERE username = ?";
    let statement = db.prepare(query).bind(&[username.into()])?;

    let result = statement.first::<serde_json::Value>(None).await?;

    let actor_data = match result {
        Some(data) => data,
        None => {
            console_log!("Actor not found: {}", username);
            return Response::error("Actor not found", 404);
        }
    };

    console_log!("Found actor: {:?}", actor_data);

    // Extract fields
    let actor_username = actor_data["username"]
        .as_str()
        .ok_or("Missing username")?;

    let public_key_pem = actor_data["public_key"]
        .as_str()
        .ok_or("Missing public key")?;

    // Build Person object
    let mut person = Person::new(
        format!("https://social.dais.social/users/{}", actor_username),
        actor_username.to_string(),
        "social.dais.social".to_string(),
        public_key_pem.to_string(),
    );

    // Add optional fields if present
    if let Some(name) = actor_data["display_name"].as_str() {
        if !name.is_empty() {
            person = person.with_name(name.to_string());
        }
    }

    if let Some(summary) = actor_data["summary"].as_str() {
        if !summary.is_empty() {
            person = person.with_summary(summary.to_string());
        }
    }

    Ok(Response::from_json(&person)?.with_headers(headers))
}
