//! dais-shared — primitives shared by `dais-core` (the Cloudflare Workers, wasm32)
//! and the `dais` client (native CLI + TUI).
//!
//! This crate is the single audited home for signing and crypto: HTTP Signatures,
//! ActivityPub wire types, and end-to-end encryption. Keeping them here (rather than
//! duplicated per front-end) is the security win called out in
//! `docs/design/CLIENT_REDESIGN.md` §2 — encryption and signing live in exactly one
//! place, used by both the server and the client.
//!
//! It is intentionally portable: no async runtime, no HTTP client, no database — so
//! it compiles unchanged for `wasm32-unknown-unknown`.

pub mod e2ee;
pub mod signatures;
pub mod types;

/// Re-export of the `rsa` crate so downstreams (the client) can derive public keys
/// etc. against the exact version dais-shared signs/encrypts with.
pub use rsa;
