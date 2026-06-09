//! dais-client — the client SDK. One brain behind the `dais` CLI and the `dais-tui`
//! TUI (CLIENT_REDESIGN.md §3): config, a local SQLite store, the Cloudflare D1 HTTP
//! API, and signing + E2EE re-exported from `dais-shared`.
//!
//! The SDK is the only layer that knows secrets — the front-ends never re-implement
//! signing or crypto.

pub mod actions;
pub mod api;
pub mod config;
pub mod d1;
pub mod e2ee;
pub mod error;
pub mod federation;
pub mod model;
pub mod platform;
pub mod signer;
pub mod store;

pub use api::{relative_time, Client, ComposeResult};
pub use config::Config;
pub use error::{Error, Result};
pub use model::{Account, Feed, FollowRequest, Post, Visibility};
pub use store::Store;
