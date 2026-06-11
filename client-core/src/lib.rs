use serde::{Deserialize, Serialize};

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
