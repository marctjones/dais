//! Domain model for the client — the types the CLI and TUI render.
//!
//! Privacy is part of the product, so [`Visibility`] and the `encrypted` flag are
//! first-class on every [`Post`] (CLIENT_REDESIGN.md §4.2/§5.2).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Post audience. Maps to the on-row glyphs in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Followers,
    Direct,
}

impl Visibility {
    /// The privacy glyph shown on each row (§5.2).
    pub fn glyph(self) -> &'static str {
        match self {
            Visibility::Public => "🌐",
            Visibility::Followers => "👥",
            Visibility::Direct => "✉",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Followers => "followers",
            Visibility::Direct => "direct",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.label()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "public" | "world" => Some(Visibility::Public),
            "followers" | "private" | "follower" => Some(Visibility::Followers),
            "direct" | "dm" => Some(Visibility::Direct),
            _ => None,
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Visibility::Followers => Visibility::Public,
            Visibility::Public => Visibility::Direct,
            Visibility::Direct => Visibility::Followers,
        }
    }
}

/// Which feed a stored post belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Feed {
    Home,
    Mentions,
    Sent,
}

impl Feed {
    pub fn as_str(self) -> &'static str {
        match self {
            Feed::Home => "home",
            Feed::Mentions => "mentions",
            Feed::Sent => "sent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "home" => Some(Feed::Home),
            "mentions" => Some(Feed::Mentions),
            "sent" => Some(Feed::Sent),
            _ => None,
        }
    }
}

/// A post in a timeline (the local mirror of inbox-ingested content, #63).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub author_handle: String,
    pub author_name: Option<String>,
    pub content: String,
    pub visibility: Visibility,
    pub encrypted: bool,
    pub published: DateTime<Utc>,
    pub in_reply_to: Option<String>,
    pub reply_count: u32,
    pub like_count: u32,
    pub boost_count: u32,
    /// Mutual follow (#64) — rendered as ★.
    pub is_friend: bool,
    /// Unread (●) vs read (○) — the email model (§5.1).
    pub unread: bool,
}

impl Post {
    /// Best display name for the author.
    pub fn display_name(&self) -> &str {
        self.author_name.as_deref().unwrap_or(&self.author_handle)
    }
}

/// An incoming follow request — the approval inbox central to private mode (§5.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowRequest {
    pub handle: String,
    pub name: Option<String>,
    pub message: Option<String>,
    pub asked_at: DateTime<Utc>,
    pub mutuals: u32,
    pub account_age_days: Option<u32>,
    pub post_count: Option<u32>,
    pub unread: bool,
}

/// Your own account profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub handle: String,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub instance: String,
}
