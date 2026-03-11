use worker::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

mod did;
mod xrpc;
mod auth;
mod relay_subscription;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        // DID document
        .get_async("/.well-known/did.json", |req, ctx| async move {
            did::handle_did_document(req, ctx).await
        })

        // XRPC endpoints
        .post_async("/xrpc/com.atproto.server.createSession", |req, ctx| async move {
            xrpc::create_session(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.server.getSession", |req, ctx| async move {
            xrpc::get_session(req, ctx).await
        })
        .post_async("/xrpc/com.atproto.repo.createRecord", |req, ctx| async move {
            xrpc::create_record(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.repo.listRecords", |req, ctx| async move {
            xrpc::list_records(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.repo.getRecord", |req, ctx| async move {
            xrpc::get_record(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.sync.getRepo", |req, ctx| async move {
            xrpc::get_repo(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.sync.listRepos", |req, ctx| async move {
            xrpc::list_repos(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.sync.getRepoStatus", |req, ctx| async move {
            xrpc::get_repo_status(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.sync.subscribeRepos", |req, ctx| async move {
            xrpc::subscribe_repos(req, ctx).await
        })
        .get_async("/xrpc/com.atproto.server.describeServer", |req, ctx| async move {
            xrpc::describe_server(req, ctx).await
        })

        // Health check
        .get("/", |_, _| {
            Response::ok("dais PDS (AT Protocol Personal Data Server)")
        })

        .run(req, env)
        .await
}
