//! ActivityPub type definitions.
//!
//! The definitions now live in the `dais-shared` crate so the server and the client
//! speak identical wire types. This module re-exports them, preserving every existing
//! `crate::activitypub::types::*` path in the codebase.

pub use dais_shared::types::*;
