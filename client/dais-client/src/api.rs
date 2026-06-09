//! The high-level client facade — one brain behind the CLI and the TUI
//! (CLIENT_REDESIGN.md §3). Holds config + local store and lazily builds the D1
//! client / signer when credentials are present.

use chrono::{Duration, Utc};

use crate::config::Config;
use crate::d1::D1Client;
use crate::error::Result;
use crate::model::{Feed, FollowRequest, Post, Visibility};
use crate::signer::Signer;
use crate::store::Store;

pub struct Client {
    pub config: Config,
    pub store: Store,
}

impl Client {
    /// Open the client with the user's config and local store.
    pub fn open() -> Result<Self> {
        let config = Config::load()?;
        let store = Store::open(&Config::store_path()?)?;
        Ok(Client { config, store })
    }

    /// Open against an explicit config (used by the TUI / tests).
    pub fn with_config(config: Config) -> Result<Self> {
        let store = Store::open(&Config::store_path()?)?;
        Ok(Client { config, store })
    }

    /// Build a client from explicit parts — used by tests with an in-memory store.
    pub fn from_parts(config: Config, store: Store) -> Self {
        Client { config, store }
    }

    /// A D1 client if credentials are configured.
    pub fn d1(&self) -> Result<D1Client> {
        D1Client::from_config(&self.config.d1)
    }

    /// A signer if a key is configured.
    pub fn signer(&self) -> Result<Signer> {
        Signer::from_config(&self.config)
    }

    pub fn is_configured(&self) -> bool {
        self.config.handle.is_some()
    }

    // ---- timeline reads (local store) ------------------------------------

    pub fn timeline(&self, feed: Feed, limit: usize) -> Result<Vec<Post>> {
        self.store.timeline(feed, limit)
    }

    pub fn thread(&self, id: &str) -> Result<(Option<Post>, Vec<Post>)> {
        let root = self.store.get_post(id)?;
        let replies = self.store.replies(id)?;
        Ok((root, replies))
    }

    pub fn requests(&self) -> Result<Vec<FollowRequest>> {
        self.store.requests()
    }

    // ---- compose ---------------------------------------------------------

    /// Stage a post locally as a draft. (Wire delivery via the worker is a later
    /// phase — this is the compose/encrypt half the client owns end-to-end.)
    pub fn compose(
        &self,
        content: &str,
        visibility: Visibility,
        encrypt: bool,
        reply_to: Option<&str>,
    ) -> Result<ComposeResult> {
        let draft_id = self
            .store
            .save_draft(content, visibility, encrypt, reply_to)?;

        let encrypted_preview = if encrypt {
            let (_enc, fallback) = crate::e2ee::encrypt_to_self(&self.config, content, None)?;
            Some(fallback)
        } else {
            None
        };

        Ok(ComposeResult {
            draft_id,
            visibility,
            encrypt,
            encrypted_preview,
        })
    }

    // ---- demo seed -------------------------------------------------------

    /// Populate the local store with the CLIENT_REDESIGN.md §5.2 sample feed so a
    /// fresh install has something to render. Idempotent (upserts by id).
    pub fn seed_demo(&self) -> Result<()> {
        let now = Utc::now();
        let posts = vec![
            Post {
                id: "demo:1".into(),
                author_handle: "@alice@coolhost.social".into(),
                author_name: Some("Alice".into()),
                content: "Morning! Anyone else watching the launch today? Coffee in hand ☕".into(),
                visibility: Visibility::Followers,
                encrypted: false,
                published: now - Duration::minutes(2),
                in_reply_to: None,
                reply_count: 4,
                like_count: 12,
                boost_count: 3,
                is_friend: true,
                unread: true,
            },
            Post {
                id: "demo:2".into(),
                author_handle: "@bob@mastodon.social".into(),
                author_name: Some("Bob Martinez".into()),
                content: "Shipped v2 of the thing. Notes: https://example.com/v2".into(),
                visibility: Visibility::Public,
                encrypted: false,
                published: now - Duration::minutes(14),
                in_reply_to: None,
                reply_count: 0,
                like_count: 30,
                boost_count: 8,
                is_friend: false,
                unread: false,
            },
            Post {
                id: "demo:3".into(),
                author_handle: "@carol@dais.carol.me".into(),
                author_name: Some("Carol".into()),
                content: "🔒 Encrypted — press ⏎ to decrypt and read in dais".into(),
                visibility: Visibility::Followers,
                encrypted: true,
                published: now - Duration::hours(1),
                in_reply_to: None,
                reply_count: 0,
                like_count: 0,
                boost_count: 0,
                is_friend: false,
                unread: true,
            },
        ];
        for p in &posts {
            self.store.upsert_post(Feed::Home, p)?;
        }

        // A reply, so `dais thread demo:1` shows something.
        self.store.upsert_post(
            Feed::Home,
            &Post {
                id: "demo:1:r1".into(),
                author_handle: self
                    .config
                    .handle
                    .clone()
                    .unwrap_or_else(|| "@you@dais.social".into()),
                author_name: Some("You".into()),
                content: "Same, can't wait. Did you see the new build notes?".into(),
                visibility: Visibility::Followers,
                encrypted: false,
                published: now - Duration::minutes(1),
                in_reply_to: Some("demo:1".into()),
                reply_count: 0,
                like_count: 0,
                boost_count: 0,
                is_friend: true,
                unread: false,
            },
        )?;

        self.store.upsert_request(&FollowRequest {
            handle: "@dave@someserver.social".into(),
            name: Some("Dave Park".into()),
            message: Some("Met you at the conf — following along!".into()),
            asked_at: now - Duration::hours(3),
            mutuals: 3,
            account_age_days: Some(730),
            post_count: Some(412),
            unread: true,
        })?;

        Ok(())
    }
}

/// Result of staging a post.
pub struct ComposeResult {
    pub draft_id: i64,
    pub visibility: Visibility,
    pub encrypt: bool,
    /// What non-dais recipients would see, when encrypting.
    pub encrypted_preview: Option<String>,
}

/// A best-effort relative-time formatter ("2m", "14m", "1h", "3d").
pub fn relative_time(then: chrono::DateTime<Utc>) -> String {
    let secs = (Utc::now() - then).num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("handle", &self.config.handle)
            .finish()
    }
}
