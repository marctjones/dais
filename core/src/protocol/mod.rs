use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtocolId {
    ActivityPub,
    Atproto,
}

impl ProtocolId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ActivityPub => "activitypub",
            Self::Atproto => "atproto",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    PublicBroadcast,
    PrivateAudience,
    DirectMessage,
    E2eeDm,
    Media,
    Threading,
    Reactions,
    Edit,
    Delete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySet {
    pub public_broadcast: bool,
    pub private_audience: bool,
    pub direct_message: bool,
    pub e2ee_dm: bool,
    pub media: bool,
    pub threading: bool,
    pub reactions: bool,
    pub edit: bool,
    pub delete: bool,
}

impl CapabilitySet {
    pub fn supports(self, capability: Capability) -> bool {
        match capability {
            Capability::PublicBroadcast => self.public_broadcast,
            Capability::PrivateAudience => self.private_audience,
            Capability::DirectMessage => self.direct_message,
            Capability::E2eeDm => self.e2ee_dm,
            Capability::Media => self.media,
            Capability::Threading => self.threading,
            Capability::Reactions => self.reactions,
            Capability::Edit => self.edit,
            Capability::Delete => self.delete,
        }
    }

    pub fn activitypub() -> Self {
        Self {
            public_broadcast: true,
            private_audience: true,
            direct_message: true,
            e2ee_dm: false,
            media: true,
            threading: true,
            reactions: true,
            edit: true,
            delete: true,
        }
    }

    pub fn atproto_public() -> Self {
        Self {
            public_broadcast: true,
            private_audience: false,
            direct_message: false,
            e2ee_dm: false,
            media: true,
            threading: true,
            reactions: true,
            edit: false,
            delete: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Public,
    Friends,
    Direct { recipients: Vec<String> },
}

impl Audience {
    pub fn required_capability(&self) -> Capability {
        match self {
            Self::Public => Capability::PublicBroadcast,
            Self::Friends => Capability::PrivateAudience,
            Self::Direct { .. } => Capability::DirectMessage,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtocolIdentity {
    pub id: String,
    pub handle: Option<String>,
    pub protocol: ProtocolId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PostIntent {
    pub content: String,
    pub audience: Audience,
    pub media: Vec<String>,
    pub in_reply_to: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageIntent {
    pub recipients: Vec<ProtocolIdentity>,
    pub body: String,
    pub encrypted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimelineCursor {
    pub before: Option<String>,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub id: String,
    pub protocol: ProtocolId,
    pub author: ProtocolIdentity,
    pub content: String,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublishReceipt {
    pub protocol: ProtocolId,
    pub local_id: String,
    pub remote_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteDrop {
    pub protocol: ProtocolId,
    pub missing_capability: Capability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutePlan {
    pub selected: Vec<ProtocolId>,
    pub dropped: Vec<RouteDrop>,
}

pub fn route_post<'a>(
    adapters: impl IntoIterator<Item = &'a dyn ProtocolAdapter>,
    post: &PostIntent,
) -> RoutePlan {
    let required = post.audience.required_capability();
    let mut selected = Vec::new();
    let mut dropped = Vec::new();

    for adapter in adapters {
        let protocol = adapter.id();
        if adapter.capabilities().supports(required) {
            selected.push(protocol);
        } else {
            dropped.push(RouteDrop {
                protocol,
                missing_capability: required,
            });
        }
    }

    RoutePlan { selected, dropped }
}

#[async_trait(?Send)]
pub trait ProtocolAdapter {
    fn id(&self) -> ProtocolId;
    fn capabilities(&self) -> CapabilitySet;

    async fn publish(&self, _post: &PostIntent) -> CoreResult<PublishReceipt> {
        Err(not_wired(self.id(), "publish"))
    }

    async fn withdraw(&self, _receipt: &PublishReceipt) -> CoreResult<()> {
        Err(not_wired(self.id(), "withdraw"))
    }

    async fn fetch_timeline(&self, _cursor: TimelineCursor) -> CoreResult<Vec<TimelineItem>> {
        Err(not_wired(self.id(), "fetch_timeline"))
    }

    async fn follow(&self, _who: &ProtocolIdentity) -> CoreResult<()> {
        Err(not_wired(self.id(), "follow"))
    }

    async fn accept_follow(&self, _who: &ProtocolIdentity) -> CoreResult<()> {
        Err(not_wired(self.id(), "accept_follow"))
    }

    async fn send_dm(&self, _message: &MessageIntent) -> CoreResult<()> {
        Err(not_wired(self.id(), "send_dm"))
    }

    async fn resolve_identity(&self, _handle: &str) -> CoreResult<ProtocolIdentity> {
        Err(not_wired(self.id(), "resolve_identity"))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ActivityPubAdapter;

#[async_trait(?Send)]
impl ProtocolAdapter for ActivityPubAdapter {
    fn id(&self) -> ProtocolId {
        ProtocolId::ActivityPub
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::activitypub()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AtprotoAdapter;

#[async_trait(?Send)]
impl ProtocolAdapter for AtprotoAdapter {
    fn id(&self) -> ProtocolId {
        ProtocolId::Atproto
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::atproto_public()
    }
}

fn not_wired(protocol: ProtocolId, operation: &str) -> CoreError {
    CoreError::Internal(format!(
        "{} adapter operation '{}' requires a platform adapter implementation",
        protocol.as_str(),
        operation
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        route_post, ActivityPubAdapter, AtprotoAdapter, Audience, Capability, PostIntent,
        ProtocolAdapter, ProtocolId,
    };

    fn post(audience: Audience) -> PostIntent {
        PostIntent {
            content: "hello".to_string(),
            audience,
            media: Vec::new(),
            in_reply_to: None,
        }
    }

    fn adapters() -> Vec<Box<dyn ProtocolAdapter>> {
        vec![Box::new(ActivityPubAdapter), Box::new(AtprotoAdapter)]
    }

    #[test]
    fn public_posts_route_to_activitypub_and_atproto() {
        let adapters = adapters();
        let adapters = adapters
            .iter()
            .map(|adapter| adapter.as_ref())
            .collect::<Vec<_>>();
        let plan = route_post(adapters, &post(Audience::Public));

        assert_eq!(
            plan.selected,
            vec![ProtocolId::ActivityPub, ProtocolId::Atproto]
        );
        assert!(plan.dropped.is_empty());
    }

    #[test]
    fn friends_posts_drop_public_only_atproto() {
        let adapters = adapters();
        let adapters = adapters
            .iter()
            .map(|adapter| adapter.as_ref())
            .collect::<Vec<_>>();
        let plan = route_post(adapters, &post(Audience::Friends));

        assert_eq!(plan.selected, vec![ProtocolId::ActivityPub]);
        assert_eq!(plan.dropped.len(), 1);
        assert_eq!(plan.dropped[0].protocol, ProtocolId::Atproto);
        assert_eq!(
            plan.dropped[0].missing_capability,
            Capability::PrivateAudience
        );
    }

    #[test]
    fn direct_messages_require_direct_message_capability() {
        let adapters = adapters();
        let adapters = adapters
            .iter()
            .map(|adapter| adapter.as_ref())
            .collect::<Vec<_>>();
        let plan = route_post(
            adapters,
            &post(Audience::Direct {
                recipients: vec!["https://example.com/users/alice".to_string()],
            }),
        );

        assert_eq!(plan.selected, vec![ProtocolId::ActivityPub]);
        assert_eq!(plan.dropped.len(), 1);
        assert_eq!(
            plan.dropped[0].missing_capability,
            Capability::DirectMessage
        );
    }
}
