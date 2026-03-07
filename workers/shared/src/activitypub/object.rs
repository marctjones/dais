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
