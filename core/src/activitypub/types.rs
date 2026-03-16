/// ActivityPub type definitions
///
/// These types represent ActivityPub objects, activities, and actors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ActivityPub Activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub activity_type: String,

    pub id: String,
    pub actor: String,
    pub object: Option<serde_json::Value>,
    pub target: Option<String>,
    pub to: Option<Vec<String>>,
    pub cc: Option<Vec<String>>,
    pub published: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// ActivityPub Actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub actor_type: String,

    pub id: String,
    pub inbox: String,
    pub outbox: String,
    pub following: Option<String>,
    pub followers: Option<String>,

    #[serde(rename = "preferredUsername")]
    pub preferred_username: String,

    pub name: Option<String>,
    pub summary: Option<String>,
    pub icon: Option<Image>,
    pub image: Option<Image>,

    #[serde(rename = "publicKey")]
    pub public_key: PublicKey,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// ActivityPub Object (Note, Article, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Object {
    #[serde(rename = "@context")]
    pub context: Option<Context>,

    #[serde(rename = "type")]
    pub object_type: String,

    pub id: String,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub published: Option<String>,
    pub updated: Option<String>,

    #[serde(rename = "attributedTo")]
    pub attributed_to: Option<String>,

    pub to: Option<Vec<String>>,
    pub cc: Option<Vec<String>>,

    #[serde(rename = "inReplyTo")]
    pub in_reply_to: Option<String>,

    pub attachment: Option<Vec<Attachment>>,
    pub tag: Option<Vec<Tag>>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Public key for actor signatures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey {
    pub id: String,
    pub owner: String,

    #[serde(rename = "publicKeyPem")]
    pub public_key_pem: String,
}

/// Image (icon or header image)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    #[serde(rename = "type")]
    pub image_type: String,

    #[serde(rename = "mediaType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    pub url: String,
}

/// ActivityPub Person actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub actor_type: String,

    pub id: String,

    #[serde(rename = "preferredUsername")]
    pub preferred_username: String,

    pub name: Option<String>,

    pub summary: Option<String>,

    pub inbox: String,

    pub outbox: String,

    pub followers: String,

    pub following: String,

    #[serde(rename = "publicKey")]
    pub public_key: PublicKey,

    pub icon: Option<Image>,

    pub image: Option<Image>,

    pub url: Option<String>,

    #[serde(rename = "manuallyApprovesFollowers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manually_approves_followers: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

impl Person {
    /// Create a new Person actor
    pub fn new(
        _id: String,
        username: String,
        domain: String,
        public_key_pem: String,
    ) -> Self {
        let base_url = format!("https://{}/users/{}", domain, username);

        Self {
            context: Context::default(),
            actor_type: "Person".to_string(),
            id: base_url.clone(),
            preferred_username: username.clone(),
            name: None,
            summary: None,
            inbox: format!("{}/inbox", base_url),
            outbox: format!("{}/outbox", base_url),
            followers: format!("{}/followers", base_url),
            following: format!("{}/following", base_url),
            public_key: PublicKey {
                id: format!("{}#main-key", base_url),
                owner: base_url.clone(),
                public_key_pem,
            },
            icon: None,
            image: None,
            url: Some(format!("https://{domain}/@{username}")),
            manually_approves_followers: Some(true),
            published: None,
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Set the bio/summary
    pub fn with_summary(mut self, summary: String) -> Self {
        self.summary = Some(summary);
        self
    }

    /// Set avatar image
    pub fn with_icon(mut self, url: String) -> Self {
        self.icon = Some(Image {
            image_type: "Image".to_string(),
            media_type: Some("image/png".to_string()),
            url,
        });
        self
    }

    /// Set header image
    pub fn with_header(mut self, url: String) -> Self {
        self.image = Some(Image {
            image_type: "Image".to_string(),
            media_type: Some("image/png".to_string()),
            url,
        });
        self
    }
}

/// Ordered Collection (for followers/following)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub collection_type: String,

    pub id: String,

    #[serde(rename = "totalItems")]
    pub total_items: u64,

    pub first: Option<String>,
}

/// Ordered Collection Page (for paginated followers/following)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedCollectionPage {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub collection_type: String,

    pub id: String,

    #[serde(rename = "partOf")]
    pub part_of: String,

    #[serde(rename = "orderedItems")]
    pub ordered_items: Vec<String>,
}

/// Attachment (media, link, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(rename = "type")]
    pub attachment_type: String,

    #[serde(rename = "mediaType")]
    pub media_type: Option<String>,

    pub url: String,
    pub name: Option<String>,

    #[serde(rename = "blurhash")]
    pub blurhash: Option<String>,

    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Tag (hashtag, mention, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    #[serde(rename = "type")]
    pub tag_type: String,

    pub name: String,
    pub href: Option<String>,
}

/// ActivityPub context
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Context {
    Single(String),
    Multiple(Vec<serde_json::Value>),
}

impl Default for Context {
    fn default() -> Self {
        Context::Single("https://www.w3.org/ns/activitystreams".to_string())
    }
}

// TODO: Add more ActivityPub types as needed
