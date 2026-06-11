use serde::{Deserialize, Serialize};

pub type ClientResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct OwnerApiClient {
    base_url: String,
    owner_token: String,
    http: reqwest::Client,
}

impl OwnerApiClient {
    pub fn new(base_url: impl Into<String>, owner_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            owner_token: owner_token.into(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn snapshot(&self) -> ClientResult<OwnerSnapshot> {
        self.get("/api/dais/owner/snapshot").await
    }

    pub async fn create_post(&self, draft: &ComposeDraft) -> ClientResult<OwnerCreatedPost> {
        self.post("/api/dais/owner/posts", draft).await
    }

    async fn get<T>(&self, path: &str) -> ClientResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.owner_token)
            .send()
            .await?;
        decode_response(response).await
    }

    async fn post<T, B>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize + ?Sized,
    {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.owner_token)
            .json(body)
            .send()
            .await?;
        decode_response(response).await
    }
}

async fn decode_response<T>(response: reqwest::Response) -> ClientResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("owner API returned {status}: {body}").into());
    }
    Ok(response.json::<T>().await?)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OwnerSection {
    Home,
    Posts,
    Sources,
    Notifications,
    Followers,
    Profile,
    Moderation,
    Deliveries,
    Settings,
    Diagnostics,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Unlisted,
    Followers,
    Direct,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProtocolRoute {
    ActivityPub,
    AtProto,
    Both,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSettings {
    pub instance_url: String,
    pub owner_token_present: bool,
    pub default_visibility: Visibility,
    pub default_protocol: ProtocolRoute,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComposeDraft {
    pub text: String,
    pub visibility: Visibility,
    pub protocol: ProtocolRoute,
    pub encrypt: bool,
    pub recipients: Vec<String>,
    pub attachments: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerPost {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub visibility: Visibility,
    pub protocol: ProtocolRoute,
    pub encrypted: bool,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerCreatedPost {
    pub id: String,
    pub actor_id: String,
    pub content: String,
    pub content_html: String,
    pub visibility: String,
    pub protocol: String,
    pub published_at: String,
    pub delivery_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceItem {
    pub id: String,
    pub title: String,
    pub source_type: String,
    pub canonical_url: Option<String>,
    pub excerpt: Option<String>,
    pub rights_policy_json: String,
    pub read: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModerationState {
    pub closed_network: bool,
    pub block_count: u64,
    pub allowlist_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticStatus {
    pub key: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSnapshot {
    pub settings: OwnerSettings,
    pub active_section: OwnerSection,
    pub posts: Vec<OwnerPost>,
    pub sources: Vec<SourceItem>,
    pub moderation: ModerationState,
    pub diagnostics: Vec<DiagnosticStatus>,
}

pub fn privacy_badges(draft: &ComposeDraft) -> Vec<&'static str> {
    let mut badges = Vec::new();
    match draft.visibility {
        Visibility::Public => badges.push("public"),
        Visibility::Unlisted => badges.push("unlisted"),
        Visibility::Followers => badges.push("private"),
        Visibility::Direct => badges.push("direct"),
    }
    match draft.protocol {
        ProtocolRoute::ActivityPub => badges.push("activitypub"),
        ProtocolRoute::AtProto => badges.push("bluesky"),
        ProtocolRoute::Both => badges.push("dual-protocol"),
    }
    if draft.encrypt {
        badges.push("e2ee");
    }
    badges
}

pub fn route_warning(draft: &ComposeDraft) -> Option<&'static str> {
    match (&draft.visibility, &draft.protocol) {
        (Visibility::Public, ProtocolRoute::AtProto | ProtocolRoute::Both) => {
            Some("Public Bluesky routing is visible outside the private ActivityPub audience.")
        }
        (Visibility::Direct, ProtocolRoute::AtProto | ProtocolRoute::Both) => {
            Some("Direct posts cannot be represented on Bluesky; route ActivityPub only.")
        }
        (Visibility::Followers | Visibility::Unlisted, ProtocolRoute::AtProto | ProtocolRoute::Both) => {
            Some("Private ActivityPub visibility is not representable on Bluesky.")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_activitypub_draft_has_private_badge_and_no_warning() {
        let draft = ComposeDraft {
            text: "hello".to_string(),
            visibility: Visibility::Followers,
            protocol: ProtocolRoute::ActivityPub,
            encrypt: false,
            recipients: Vec::new(),
            attachments: Vec::new(),
        };
        assert_eq!(privacy_badges(&draft), vec!["private", "activitypub"]);
        assert_eq!(route_warning(&draft), None);
    }

    #[test]
    fn direct_bluesky_route_warns() {
        let draft = ComposeDraft {
            text: "secret".to_string(),
            visibility: Visibility::Direct,
            protocol: ProtocolRoute::Both,
            encrypt: true,
            recipients: vec!["https://example.com/users/alice".to_string()],
            attachments: Vec::new(),
        };
        assert!(privacy_badges(&draft).contains(&"e2ee"));
        assert_eq!(
            route_warning(&draft),
            Some("Direct posts cannot be represented on Bluesky; route ActivityPub only.")
        );
    }

    #[test]
    fn snapshot_serializes_for_tauri_commands() {
        let snapshot = OwnerSnapshot {
            settings: OwnerSettings {
                instance_url: "https://social.dais.social".to_string(),
                owner_token_present: true,
                default_visibility: Visibility::Followers,
                default_protocol: ProtocolRoute::Both,
            },
            active_section: OwnerSection::Home,
            posts: Vec::new(),
            sources: Vec::new(),
            moderation: ModerationState {
                closed_network: false,
                block_count: 0,
                allowlist_count: 0,
            },
            diagnostics: vec![DiagnosticStatus {
                key: "owner-api".to_string(),
                ok: false,
                detail: "not configured".to_string(),
            }],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("social.dais.social"));
    }
}
