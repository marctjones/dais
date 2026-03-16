/// Cloudflare platform bindings for dais
///
/// This library provides Cloudflare-specific implementations of the dais-core
/// platform traits, allowing dais to run on Cloudflare Workers.
///
/// ## Platform Providers
///
/// - **D1Provider** - Database provider using Cloudflare D1 (SQLite)
/// - **R2Provider** - Storage provider using Cloudflare R2
/// - **CloudflareQueueProvider** - Queue provider using Cloudflare Queues
/// - **WorkerHttpProvider** - HTTP provider using Workers fetch API
///
/// ## Usage
///
/// ```rust,ignore
/// use worker::*;
/// use dais_core::DaisCore;
/// use dais_cloudflare::{D1Provider, R2Provider, CloudflareQueueProvider, WorkerHttpProvider};
///
/// #[event(fetch)]
/// async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
///     // Create platform providers
///     let db = D1Provider::new(env.d1("DB")?);
///     let storage = R2Provider::new(env.r2("MEDIA")?, "https://media.dais.social");
///     let queue = CloudflareQueueProvider::new(env.queue("delivery")?);
///     let http = WorkerHttpProvider::new();
///
///     // Create dais core
///     let core = DaisCore::new(
///         Box::new(db),
///         Box::new(storage),
///         Box::new(queue),
///         Box::new(http),
///         config,
///     );
///
///     // Use core methods
///     core.handle_inbox(actor, activity).await?;
///
///     Response::ok("OK")
/// }
/// ```

pub mod d1;
// pub mod r2;  // TODO: Re-enable when R2 API is available in worker-rs
pub mod queues;
pub mod http;

pub use d1::D1Provider;
// pub use r2::R2Provider;
pub use queues::CloudflareQueueProvider;
pub use http::WorkerHttpProvider;

// Re-export core types for convenience
pub use dais_core::{
    DaisCore, CoreConfig, CoreResult, CoreError,
    DatabaseProvider, StorageProvider, QueueProvider, HttpProvider,
};
