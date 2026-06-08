//! ActivityPub types and structures
//!
//! Implements ActivityPub protocol types according to:
//! https://www.w3.org/TR/activitypub/
//! https://www.w3.org/TR/activitystreams-core/

pub mod actor;
pub mod activity;
pub mod object;

pub use actor::*;
pub use activity::*;
pub use object::*;

/// Common JSON-LD context for ActivityPub
pub const ACTIVITY_STREAMS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
pub const SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";

/// Get the standard ActivityPub context array
pub fn activitypub_context() -> serde_json::Value {
    serde_json::json!([
        ACTIVITY_STREAMS_CONTEXT,
        SECURITY_CONTEXT
    ])
}
