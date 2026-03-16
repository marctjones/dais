/// Refactored Router worker
///
/// This worker routes requests to the appropriate backend worker based on the path.
/// It acts as a single entry point for all dais requests.
///
/// Routes:
/// - /.well-known/webfinger -> webfinger worker
/// - /users/:username/inbox -> inbox worker
/// - /users/:username/outbox -> outbox worker
/// - /users/:username/posts/:id -> outbox worker
/// - /users/:username/followers -> actor worker
/// - /users/:username/following -> actor worker
/// - /users/:username -> actor worker
/// - /auth/* -> auth worker
/// - /xrpc/* -> pds worker (AT Protocol)
/// - / -> landing worker

use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();

    console_log!("Router: {} {}", req.method(), path);

    // Route to appropriate worker based on path
    let service_name = if path.starts_with("/.well-known/webfinger") {
        "webfinger"
    } else if path.starts_with("/auth") {
        "auth"
    } else if path.contains("/inbox") {
        "inbox"
    } else if path.contains("/outbox") || path.contains("/posts/") {
        "outbox"
    } else if path.starts_with("/users/") {
        "actor"
    } else if path.starts_with("/xrpc") {
        "pds"
    } else {
        "landing"
    };

    console_log!("Routing to: {}", service_name);

    // For now, just return a simple response indicating the route
    // TODO: Implement service bindings when worker-rs API is available
    // Service bindings require wrangler.toml configuration
    Response::ok(format!("Would route to: {} (service bindings not yet configured)", service_name))
}
