// Vercel Edge Functions bindings for dais
//
// This library provides Vercel-specific implementations of the dais-core traits,
// enabling deployment to Vercel Edge Functions.

mod neon;
mod blob;
mod http;
mod queue;

pub use neon::NeonProvider;
pub use blob::VercelBlobProvider;
pub use http::VercelHttpProvider;
pub use queue::VercelQueueProvider;

// Re-export core types for convenience
pub use dais_core::traits::*;
pub use dais_core::types::*;
pub use dais_core::DaisCore;
