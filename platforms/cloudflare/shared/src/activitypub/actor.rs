///! ActivityPub Actor types (Person, Service, etc.)

use serde::{Deserialize, Serialize};

/// ActivityPub Person actor
///
/// Represents a person in the fediverse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    #[serde(rename = "@context")]
    pub context: serde_json::Value,

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
            context: super::activitypub_context(),
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

/// Public key object for HTTP signature verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey {
    pub id: String,
    pub owner: String,

    #[serde(rename = "publicKeyPem")]
    pub public_key_pem: String,
}

/// Image object for avatars and headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    #[serde(rename = "type")]
    pub image_type: String,

    #[serde(rename = "mediaType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    pub url: String,
}
