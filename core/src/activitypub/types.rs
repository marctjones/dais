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
    pub media_type: Option<String>,

    pub url: String,
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
