///! ActivityPub Object types (Note, etc.)

use serde::{Deserialize, Serialize};

/// ActivityPub Note (a post/status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    #[serde(rename = "@context")]
    pub context: serde_json::Value,

    #[serde(rename = "type")]
    pub note_type: String,

    pub id: String,

    pub attributed_to: String,

    pub content: String,

    pub published: String,

    #[serde(rename = "to")]
    pub to: Vec<String>,

    #[serde(rename = "cc")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Vec<String>>,

    #[serde(rename = "inReplyTo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Vec<Attachment>>,
}

impl Note {
    /// Create a new public note
    pub fn public(id: String, actor: String, content: String) -> Self {
        Self {
            context: super::activitypub_context(),
            note_type: "Note".to_string(),
            id,
            attributed_to: actor,
            content,
            published: chrono::Utc::now().to_rfc3339(),
            to: vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
            cc: None,
            in_reply_to: None,
            attachment: None,
        }
    }
}

/// Media attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(rename = "type")]
    pub attachment_type: String,

    #[serde(rename = "mediaType")]
    pub media_type: String,

    pub url: String,

    pub name: Option<String>,
}

/// ActivityPub OrderedCollection (for outbox, followers, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: serde_json::Value,

    #[serde(rename = "type")]
    pub collection_type: String,

    pub id: String,

    #[serde(rename = "totalItems")]
    pub total_items: usize,

    #[serde(rename = "orderedItems")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered_items: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
}

impl OrderedCollection {
    /// Create a new ordered collection with items
    pub fn new(id: String, items: Vec<serde_json::Value>) -> Self {
        let total = items.len();
        Self {
            context: super::activitypub_context(),
            collection_type: "OrderedCollection".to_string(),
            id,
            total_items: total,
            ordered_items: Some(items),
            first: None,
            last: None,
        }
    }

    /// Create an empty ordered collection
    pub fn empty(id: String) -> Self {
        Self {
            context: super::activitypub_context(),
            collection_type: "OrderedCollection".to_string(),
            id,
            total_items: 0,
            ordered_items: Some(vec![]),
            first: None,
            last: None,
        }
    }
}
