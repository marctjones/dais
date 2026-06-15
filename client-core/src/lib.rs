use serde::{Deserialize, Serialize};
use std::time::Duration;

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
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub async fn snapshot(&self) -> ClientResult<OwnerSnapshot> {
        self.get("/api/dais/owner/snapshot").await
    }

    pub async fn post_detail(&self, id: &str) -> ClientResult<OwnerPostDetail> {
        self.get(&format!("/api/dais/owner/posts/{}", url_encode(id)))
            .await
    }

    pub async fn discover_actor(&self, target: &str) -> ClientResult<OwnerDiscoveredActor> {
        self.post("/api/dais/owner/discovery/actor", &FollowTarget { target })
            .await
    }

    pub async fn create_post(&self, draft: &ComposeDraft) -> ClientResult<OwnerCreatedPost> {
        self.post("/api/dais/owner/posts", draft).await
    }

    pub async fn interact(
        &self,
        interaction: &OwnerInteraction,
    ) -> ClientResult<OwnerInteractionResult> {
        self.post("/api/dais/owner/interactions", interaction).await
    }

    pub async fn upload_media(&self, media: &OwnerMediaUpload) -> ClientResult<OwnerMedia> {
        self.post("/api/dais/owner/media", media).await
    }

    pub async fn set_follower_status(
        &self,
        follower_actor_id: &str,
        status: &str,
    ) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/followers/status",
            &FollowerStatusUpdate {
                follower_actor_id,
                status,
            },
        )
        .await
    }

    pub async fn update_profile(&self, profile: &OwnerProfileUpdate) -> ClientResult<OwnerProfile> {
        self.post("/api/dais/owner/profile", profile).await
    }

    pub async fn notifications(&self) -> ClientResult<Vec<OwnerNotification>> {
        let response: OwnerItems<OwnerNotification> =
            self.get("/api/dais/owner/notifications").await?;
        Ok(response.items)
    }

    pub async fn friends(&self) -> ClientResult<Vec<OwnerFriend>> {
        let response: OwnerItems<OwnerFriend> = self.get("/api/dais/owner/friends").await?;
        Ok(response.items)
    }

    pub async fn mark_notification_read(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/notifications/read",
            &NotificationRead { id },
        )
        .await
    }

    pub async fn deliveries(&self) -> ClientResult<Vec<OwnerDelivery>> {
        let response: OwnerItems<OwnerDelivery> = self.get("/api/dais/owner/deliveries").await?;
        Ok(response.items)
    }

    pub async fn direct_messages(&self) -> ClientResult<Vec<OwnerDirectMessage>> {
        let response: OwnerItems<OwnerDirectMessage> =
            self.get("/api/dais/owner/direct-messages").await?;
        Ok(response.items)
    }

    pub async fn search(&self, query: &str) -> ClientResult<OwnerSearchResult> {
        self.get(&format!("/api/dais/owner/search?q={}", url_encode(query)))
            .await
    }

    pub async fn stats(&self) -> ClientResult<OwnerStats> {
        self.get("/api/dais/owner/stats").await
    }

    pub async fn sources(&self) -> ClientResult<OwnerSources> {
        self.get("/api/dais/owner/sources").await
    }

    pub async fn add_source(&self, source: &OwnerSourceAdd) -> ClientResult<OwnerSourceAddResult> {
        self.post("/api/dais/owner/sources", source).await
    }

    pub async fn remove_source(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!("/api/dais/owner/sources/{id}")).await
    }

    pub async fn refresh_sources(
        &self,
        id: Option<&str>,
    ) -> ClientResult<OwnerSourceRefreshResult> {
        self.post(
            "/api/dais/owner/sources/refresh",
            &OwnerSourceRefresh { id },
        )
        .await
    }

    pub async fn moderation(&self) -> ClientResult<ModerationState> {
        self.get("/api/dais/owner/moderation").await
    }

    pub async fn block_actor(
        &self,
        actor_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/moderation/block",
            &ModerationBlock {
                actor_id: Some(actor_id),
                domain: None,
                reason,
            },
        )
        .await
    }

    pub async fn block_domain(
        &self,
        domain: &str,
        reason: Option<&str>,
    ) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/moderation/block",
            &ModerationBlock {
                actor_id: None,
                domain: Some(domain),
                reason,
            },
        )
        .await
    }

    pub async fn unblock(&self, value: &str) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/moderation/unblock",
            &ModerationUnblock { value },
        )
        .await
    }

    pub async fn allow_host(
        &self,
        host: &str,
        note: Option<&str>,
    ) -> ClientResult<OwnerActionResult> {
        self.post(
            "/api/dais/owner/moderation/allowlist",
            &ModerationAllow { host, note },
        )
        .await
    }

    pub async fn disallow_host(&self, host: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!("/api/dais/owner/moderation/allowlist/{host}"))
            .await
    }

    pub async fn follow_actor(&self, target: &str) -> ClientResult<OwnerFollowResult> {
        self.post("/api/dais/owner/following/follow", &FollowTarget { target })
            .await
    }

    pub async fn unfollow_actor(&self, target: &str) -> ClientResult<OwnerFollowResult> {
        self.post(
            "/api/dais/owner/following/unfollow",
            &FollowTarget { target },
        )
        .await
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

    async fn delete<T>(&self, path: &str) -> ClientResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self
            .http
            .delete(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.owner_token)
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
    pub in_reply_to: Option<String>,
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
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    #[serde(default)]
    pub reply_count: u64,
    #[serde(default)]
    pub like_count: u64,
    #[serde(default)]
    pub boost_count: u64,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerTimelinePost {
    pub id: String,
    pub object_id: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub content: String,
    pub content_html: Option<String>,
    pub visibility: String,
    pub in_reply_to: Option<String>,
    pub published_at: Option<String>,
    pub protocol: Option<String>,
    #[serde(default)]
    pub reply_count: u64,
    #[serde(default)]
    pub like_count: u64,
    #[serde(default)]
    pub boost_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerPostDetail {
    #[serde(flatten)]
    pub post: OwnerPost,
    pub content_html: Option<String>,
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub replies: Vec<serde_json::Value>,
    #[serde(default)]
    pub likes: Vec<serde_json::Value>,
    #[serde(default)]
    pub boosts: Vec<serde_json::Value>,
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
    pub in_reply_to: Option<String>,
    pub delivery_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerInteraction {
    pub object_id: String,
    pub interaction: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerInteractionResult {
    pub ok: bool,
    pub activity_id: String,
    pub interaction: String,
    pub object_id: String,
    pub delivery_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerMediaUpload {
    pub filename: String,
    pub media_type: Option<String>,
    pub access: Option<String>,
    pub data_base64: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerMedia {
    pub url: String,
    pub media_type: Option<String>,
    pub attachment: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerActionResult {
    pub ok: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FollowerStatusUpdate<'a> {
    follower_actor_id: &'a str,
    status: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FollowTarget<'a> {
    target: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NotificationRead<'a> {
    id: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct OwnerItems<T> {
    items: Vec<T>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerFollower {
    pub id: String,
    pub actor_id: String,
    pub follower_actor_id: String,
    pub follower_inbox: String,
    pub follower_shared_inbox: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerFriend {
    pub friend_actor_id: String,
    pub friend_inbox: Option<String>,
    pub friend_shared_inbox: Option<String>,
    pub follower_since: Option<String>,
    pub following_since: Option<String>,
    pub accepted_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerFollowing {
    pub id: String,
    pub actor_id: String,
    pub target_actor_id: String,
    pub target_inbox: String,
    pub status: String,
    pub created_at: Option<String>,
    pub accepted_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerFollowResult {
    pub ok: bool,
    pub following: OwnerFollowing,
    pub delivery_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerDiscoveredActor {
    pub id: String,
    pub inbox: String,
    pub shared_inbox: Option<String>,
    pub preferred_username: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub icon_url: Option<String>,
    pub handle: Option<String>,
    pub following_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerNotification {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub post_id: Option<String>,
    pub activity_id: Option<String>,
    pub content: Option<String>,
    pub read: serde_json::Value,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerDelivery {
    pub id: String,
    pub post_id: String,
    pub target_type: Option<String>,
    pub target_url: String,
    pub protocol: String,
    pub status: String,
    pub retry_count: Option<u64>,
    pub last_attempt_at: Option<String>,
    pub error_message: Option<String>,
    pub activity_type: Option<String>,
    pub created_at: Option<String>,
    pub delivered_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerDirectMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub content: String,
    pub published_at: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchResult {
    #[serde(default)]
    pub posts: Vec<OwnerSearchPost>,
    #[serde(default)]
    pub users: Vec<OwnerSearchUser>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchPost {
    pub id: String,
    pub actor_id: Option<String>,
    pub content: String,
    pub content_html: Option<String>,
    pub object_type: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<String>,
    pub poll_options: Option<String>,
    pub visibility: Option<String>,
    pub protocol: Option<String>,
    pub published_at: Option<String>,
    pub in_reply_to: Option<String>,
    pub atproto_uri: Option<String>,
    pub encrypted_message: Option<String>,
    pub media_attachments: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchUser {
    pub actor_id: String,
    pub relation: String,
    pub status: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerStats {
    pub followers_total: u64,
    pub followers_approved: u64,
    pub followers_pending: u64,
    pub followers_rejected: u64,
    pub following_total: u64,
    pub posts_total: u64,
    pub activities_total: u64,
    pub deliveries_total: u64,
    pub deliveries_failed: u64,
    pub deliveries_queued: u64,
    pub deliveries_retry: u64,
    pub deliveries_delivered: u64,
    pub dual_protocol_posts: u64,
    pub public_posts: u64,
    pub private_posts: u64,
    pub direct_posts: u64,
    pub encrypted_posts: u64,
    pub media_posts: u64,
    pub notifications_unread: u64,
    pub blocks_total: u64,
    pub allowlist_hosts: u64,
    pub closed_network: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerProfile {
    pub id: String,
    pub username: String,
    pub actor_type: String,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub icon: Option<String>,
    pub image: Option<String>,
    pub avatar_url: Option<String>,
    pub header_url: Option<String>,
    pub public_handle: String,
    pub actor_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerProfileUpdate {
    pub actor_type: Option<String>,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub icon: Option<String>,
    pub image: Option<String>,
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
pub struct SourceSubscription {
    pub id: String,
    pub source_type: String,
    pub url: String,
    pub title: Option<String>,
    pub homepage_url: Option<String>,
    pub status: String,
    pub refresh_cadence_minutes: u64,
    pub last_fetched_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub last_error: Option<String>,
    pub error_count: u64,
    pub policy_json: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSources {
    pub subscriptions: Vec<SourceSubscription>,
    pub items: Vec<SourceItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSourceAdd {
    pub source_type: String,
    pub url: String,
    pub title: Option<String>,
    pub cadence_minutes: Option<u16>,
    pub api_secret_name: Option<String>,
    pub private_reader_only: bool,
    pub excerpt_only: bool,
    pub link_required: bool,
    pub attribution_required: bool,
    pub image_allowed: bool,
    pub full_text_allowed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSourceAddResult {
    pub ok: bool,
    pub source: SourceSubscription,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OwnerSourceRefresh<'a> {
    id: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSourceRefreshItem {
    pub id: String,
    pub ok: bool,
    pub status: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSourceRefreshResult {
    pub ok: bool,
    pub items: Vec<OwnerSourceRefreshItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModerationState {
    pub closed_network: bool,
    pub block_count: u64,
    pub allowlist_count: u64,
    #[serde(default)]
    pub blocks: Vec<ModerationBlockRow>,
    #[serde(default)]
    pub allowlist: Vec<ModerationAllowlistHost>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModerationBlockRow {
    pub id: String,
    pub actor_id: String,
    pub blocked_domain: Option<String>,
    pub reason: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModerationAllowlistHost {
    pub host: String,
    pub note: Option<String>,
    pub enabled: serde_json::Value,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ModerationBlock<'a> {
    actor_id: Option<&'a str>,
    domain: Option<&'a str>,
    reason: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ModerationUnblock<'a> {
    value: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ModerationAllow<'a> {
    host: &'a str,
    note: Option<&'a str>,
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
    pub profile: OwnerProfile,
    pub home_timeline: Vec<OwnerTimelinePost>,
    pub posts: Vec<OwnerPost>,
    pub followers: Vec<OwnerFollower>,
    pub friends: Vec<OwnerFriend>,
    pub following: Vec<OwnerFollowing>,
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
        (
            Visibility::Followers | Visibility::Unlisted,
            ProtocolRoute::AtProto | ProtocolRoute::Both,
        ) => Some("Private ActivityPub visibility is not representable on Bluesky."),
        _ => None,
    }
}

fn url_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
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
            in_reply_to: None,
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
            in_reply_to: None,
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
            profile: OwnerProfile {
                id: "https://social.dais.social/users/social".to_string(),
                username: "social".to_string(),
                actor_type: "Person".to_string(),
                display_name: Some("dais".to_string()),
                summary: Some("Private-by-default social server.".to_string()),
                icon: None,
                image: None,
                avatar_url: None,
                header_url: None,
                public_handle: "@social@dais.social".to_string(),
                actor_url: "https://social.dais.social/users/social".to_string(),
            },
            home_timeline: Vec::new(),
            posts: Vec::new(),
            followers: Vec::new(),
            friends: Vec::new(),
            following: Vec::new(),
            sources: Vec::new(),
            moderation: ModerationState {
                closed_network: false,
                block_count: 0,
                allowlist_count: 0,
                blocks: Vec::new(),
                allowlist: Vec::new(),
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
