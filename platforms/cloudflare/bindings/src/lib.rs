/// Cloudflare platform bindings for dais
///
/// This library provides Cloudflare-specific implementations of the dais-core
/// platform traits, allowing dais to run on Cloudflare Workers.
///
/// ## Platform Providers
///
/// - **D1Provider** - Database provider using Cloudflare D1 (SQLite)
/// - **CloudflareQueueProvider** - Queue provider using Cloudflare Queues
/// - **WorkerHttpProvider** - HTTP provider using Workers fetch API
///
/// R2/media storage is intentionally handled in the active router worker today.
/// This crate should not expose a partial storage provider until metadata,
/// listing, signed-access, and private-media semantics can be implemented and
/// tested through the shared `StorageProvider` trait without returning dummy
/// URLs or silently dropping requested options.
///
/// ## Usage
///
/// ```rust,ignore
/// use worker::*;
/// use dais_core::DaisCore;
/// use dais_cloudflare::{D1Provider, CloudflareQueueProvider, WorkerHttpProvider};
///
/// #[event(fetch)]
/// async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
///     // Create platform providers
///     let db = D1Provider::new(env.d1("DB")?);
///     let queue = CloudflareQueueProvider::new(env.queue("delivery")?);
///     let http = WorkerHttpProvider::new();
///
///     Response::ok("OK")
/// }
/// ```
pub mod d1;
pub mod http;
pub mod queues;

pub use d1::D1Provider;
pub use http::WorkerHttpProvider;
pub use queues::CloudflareQueueProvider;

// Re-export core types for convenience
pub use dais_core::{
    CoreConfig, CoreError, CoreResult, DaisCore, DatabaseProvider, HttpProvider, QueueProvider,
    StorageProvider,
};
