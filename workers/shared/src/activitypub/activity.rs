///! ActivityPub Activity types (Follow, Accept, Create, etc.)

use serde::{Deserialize, Serialize};

/// Generic Activity structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    #[serde(rename = "@context")]
    pub context: serde_json::Value,

    #[serde(rename = "type")]
    pub activity_type: String,

    pub id: String,

    pub actor: String,

    pub object: serde_json::Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

impl Activity {
    /// Create a Follow activity
    pub fn follow(id: String, actor: String, object: String) -> Self {
        Self {
            context: super::activitypub_context(),
            activity_type: "Follow".to_string(),
            id,
            actor,
            object: serde_json::json!(object),
            target: None,
            published: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Create an Accept activity
    pub fn accept(id: String, actor: String, object: serde_json::Value) -> Self {
        Self {
            context: super::activitypub_context(),
            activity_type: "Accept".to_string(),
            id,
            actor,
            object,
            target: None,
            published: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Create a Reject activity
    pub fn reject(id: String, actor: String, object: serde_json::Value) -> Self {
        Self {
            context: super::activitypub_context(),
            activity_type: "Reject".to_string(),
            id,
            actor,
            object,
            target: None,
            published: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Create a Create activity
    pub fn create(id: String, actor: String, object: serde_json::Value) -> Self {
        Self {
            context: super::activitypub_context(),
            activity_type: "Create".to_string(),
            id,
            actor,
            object,
            target: None,
            published: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}
