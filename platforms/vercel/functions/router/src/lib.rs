use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

pub async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let path = req.uri().path();
    let method = req.method();

    // Route to appropriate function based on path
    let (target_function, description) = match (method.as_str(), path) {
        ("GET", "/.well-known/webfinger") =>
            ("webfinger", "WebFinger discovery"),

        ("GET", path) if path.starts_with("/users/") && path.ends_with("/inbox") =>
            ("inbox", "ActivityPub inbox (GET not supported)"),
        ("POST", path) if path.starts_with("/users/") && path.ends_with("/inbox") =>
            ("inbox", "ActivityPub inbox"),

        ("GET", path) if path.starts_with("/users/") && path.ends_with("/outbox") =>
            ("outbox", "ActivityPub outbox"),

        ("GET", path) if path.starts_with("/users/") && path.ends_with("/followers") =>
            ("actor", "Followers collection"),
        ("GET", path) if path.starts_with("/users/") && path.ends_with("/following") =>
            ("actor", "Following collection"),
        ("GET", path) if path.starts_with("/users/") =>
            ("actor", "Actor profile"),

        ("POST", "/auth/login") | ("POST", "/auth/verify") =>
            ("auth", "Authentication"),

        ("GET", path) if path.starts_with("/xrpc/") =>
            ("pds", "AT Protocol"),
        ("POST", path) if path.starts_with("/xrpc/") =>
            ("pds", "AT Protocol"),

        ("POST", "/queue/process") =>
            ("delivery-queue", "Process delivery queue"),

        ("GET", "/") =>
            ("landing", "Instance homepage"),

        _ => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Route not found".into())?);
        }
    };

    // In production, this would proxy to the actual function
    // For now, return routing info
    let response = serde_json::json!({
        "router": "dais-vercel",
        "path": path,
        "method": method.as_str(),
        "target_function": target_function,
        "description": description,
        "note": "This is the router function. In production, configure routes in vercel.json instead."
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(response.to_string().into())?)
}
