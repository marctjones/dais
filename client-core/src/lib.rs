pub mod e2ee;

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

    pub async fn settings(&self) -> ClientResult<OwnerSettings> {
        self.get("/api/dais/owner/settings").await
    }

    pub async fn update_settings(
        &self,
        settings: &OwnerSettingsUpdate,
    ) -> ClientResult<OwnerSettings> {
        self.post("/api/dais/owner/settings", settings).await
    }

    pub async fn home_timeline(
        &self,
        limit: usize,
        include_replies: bool,
    ) -> ClientResult<Vec<OwnerTimelinePost>> {
        let response: OwnerItems<OwnerTimelinePost> = self
            .get(&format!(
                "/api/dais/owner/timeline/home?limit={limit}&include_replies={include_replies}"
            ))
            .await?;
        Ok(response.items)
    }

    pub async fn post_detail(&self, id: &str) -> ClientResult<OwnerPostDetail> {
        self.get(&format!("/api/dais/owner/posts/{}", url_encode(id)))
            .await
    }

    pub async fn delete_post(&self, id: &str) -> ClientResult<OwnerDeletedPost> {
        self.delete(&format!("/api/dais/owner/posts/{}", url_encode(id)))
            .await
    }

    pub async fn saved_posts(&self) -> ClientResult<Vec<OwnerSavedPost>> {
        let response: OwnerItems<OwnerSavedPost> = self.get("/api/dais/owner/saved").await?;
        Ok(response.items)
    }

    pub async fn save_post(&self, post: &OwnerSavePost) -> ClientResult<OwnerSavedPost> {
        self.post("/api/dais/owner/saved", post).await
    }

    pub async fn unsave_post(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!("/api/dais/owner/saved/{}", url_encode(id)))
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

    pub async fn revoke_media(&self, url: &str) -> ClientResult<OwnerActionResult> {
        self.post("/api/dais/owner/media/revoke", &OwnerMediaRevoke { url })
            .await
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

    pub async fn followers(&self, limit: usize) -> ClientResult<Vec<OwnerFollower>> {
        let response: OwnerItems<OwnerFollower> = self
            .get(&format!("/api/dais/owner/followers?limit={limit}"))
            .await?;
        Ok(response.items)
    }

    pub async fn following(&self, limit: usize) -> ClientResult<Vec<OwnerFollowing>> {
        let response: OwnerItems<OwnerFollowing> = self
            .get(&format!("/api/dais/owner/following?limit={limit}"))
            .await?;
        Ok(response.items)
    }

    pub async fn audience_lists(&self) -> ClientResult<Vec<OwnerAudienceList>> {
        let response: OwnerItems<OwnerAudienceList> =
            self.get("/api/dais/owner/audience-lists").await?;
        Ok(response.items)
    }

    pub async fn upsert_audience_list(
        &self,
        list: &OwnerAudienceListUpsert,
    ) -> ClientResult<OwnerAudienceList> {
        self.post("/api/dais/owner/audience-lists", list).await
    }

    pub async fn delete_audience_list(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!(
            "/api/dais/owner/audience-lists/{}",
            url_encode(id)
        ))
        .await
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

    pub async fn retry_delivery(&self, id: &str) -> ClientResult<OwnerDelivery> {
        self.post(
            &format!("/api/dais/owner/deliveries/{}/retry", url_encode(id)),
            &OwnerEmptyBody {},
        )
        .await
    }

    pub async fn cancel_delivery(&self, id: &str) -> ClientResult<OwnerDelivery> {
        self.post(
            &format!("/api/dais/owner/deliveries/{}/cancel", url_encode(id)),
            &OwnerEmptyBody {},
        )
        .await
    }

    pub async fn direct_messages(&self) -> ClientResult<Vec<OwnerDirectMessage>> {
        let response: OwnerItems<OwnerDirectMessage> =
            self.get("/api/dais/owner/direct-messages").await?;
        Ok(response.items)
    }

    pub async fn e2ee_messages(&self) -> ClientResult<Vec<OwnerE2eeMessage>> {
        let response: OwnerItems<OwnerE2eeMessage> =
            self.get("/api/dais/owner/e2ee/messages").await?;
        Ok(response.items)
    }

    pub async fn delete_e2ee_message(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!("/api/dais/owner/e2ee/messages/{}", url_encode(id)))
            .await
    }

    pub async fn send_e2ee_message(
        &self,
        message: &OwnerE2eeMessageSend,
    ) -> ClientResult<OwnerE2eeMessageSendResult> {
        self.post("/api/dais/owner/e2ee/messages", message).await
    }

    pub async fn e2ee_devices(&self) -> ClientResult<Vec<OwnerE2eeDevice>> {
        let response: OwnerItems<OwnerE2eeDevice> =
            self.get("/api/dais/owner/e2ee/devices").await?;
        Ok(response.items)
    }

    pub async fn upsert_e2ee_device(
        &self,
        device: &OwnerE2eeDeviceUpsert,
    ) -> ClientResult<OwnerE2eeDevice> {
        self.post("/api/dais/owner/e2ee/devices", device).await
    }

    pub async fn revoke_e2ee_device(
        &self,
        device: &OwnerE2eeDeviceRef,
    ) -> ClientResult<OwnerE2eeDevice> {
        self.post("/api/dais/owner/e2ee/devices/revoke", device)
            .await
    }

    pub async fn e2ee_peer_devices(&self) -> ClientResult<Vec<OwnerE2eePeerDevice>> {
        let response: OwnerItems<OwnerE2eePeerDevice> =
            self.get("/api/dais/owner/e2ee/peers").await?;
        Ok(response.items)
    }

    pub async fn discover_e2ee_peer_devices(
        &self,
        peer: &OwnerE2eePeerDiscoverRequest,
    ) -> ClientResult<OwnerE2eePeerDiscoverResult> {
        self.post("/api/dais/owner/e2ee/peers/discover", peer).await
    }

    pub async fn trust_e2ee_peer_device(
        &self,
        peer: &OwnerE2eePeerTrustRequest,
    ) -> ClientResult<OwnerE2eePeerDevice> {
        self.post("/api/dais/owner/e2ee/peers/trust", peer).await
    }

    pub async fn revoke_e2ee_peer_device(
        &self,
        peer: &OwnerE2eePeerDeviceRef,
    ) -> ClientResult<OwnerE2eePeerDevice> {
        self.post("/api/dais/owner/e2ee/peers/revoke", peer).await
    }

    pub async fn search(&self, query: &str) -> ClientResult<OwnerSearchResult> {
        self.search_with_scope(query, "local").await
    }

    pub async fn search_with_scope(
        &self,
        query: &str,
        scope: &str,
    ) -> ClientResult<OwnerSearchResult> {
        self.search_with_scope_confirmation(query, scope, false)
            .await
    }

    pub async fn search_with_scope_confirmation(
        &self,
        query: &str,
        scope: &str,
        confirm_public_sensitive: bool,
    ) -> ClientResult<OwnerSearchResult> {
        self.search_with_options(&OwnerSearchQuery {
            query: query.to_string(),
            scope: scope.to_string(),
            confirm_public_sensitive,
            ..OwnerSearchQuery::default()
        })
        .await
    }

    pub async fn search_with_options(
        &self,
        options: &OwnerSearchQuery,
    ) -> ClientResult<OwnerSearchResult> {
        let mut params = vec![
            ("q", options.query.as_str()),
            ("scope", options.scope.as_str()),
        ];
        if let Some(provider) = options.provider.as_deref() {
            params.push(("provider", provider));
        }
        if let Some(result_type) = options.result_type.as_deref() {
            params.push(("type", result_type));
        }
        if let Some(sort) = options.sort.as_deref() {
            params.push(("sort", sort));
        }
        if let Some(since) = options.since.as_deref() {
            params.push(("since", since));
        }
        if let Some(until) = options.until.as_deref() {
            params.push(("until", until));
        }
        if let Some(author) = options.author.as_deref() {
            params.push(("author", author));
        }
        if let Some(mentions) = options.mentions.as_deref() {
            params.push(("mentions", mentions));
        }
        if let Some(lang) = options.lang.as_deref() {
            params.push(("lang", lang));
        }
        if let Some(domain) = options.domain.as_deref() {
            params.push(("domain", domain));
        }
        if let Some(url) = options.url.as_deref() {
            params.push(("url", url));
        }
        let mut path = format!("/api/dais/owner/search?{}", encode_query(&params));
        for server in &options.servers {
            path.push_str("&server=");
            path.push_str(&url_encode(server));
        }
        for tag in &options.tags {
            path.push_str("&tag=");
            path.push_str(&url_encode(tag));
        }
        if options.confirm_public_sensitive {
            path.push_str("&confirm_public_sensitive=true");
        }
        self.get(&path).await
    }

    pub async fn stats(&self) -> ClientResult<OwnerStats> {
        self.get("/api/dais/owner/stats").await
    }

    pub async fn diagnostics(&self) -> ClientResult<Vec<DiagnosticStatus>> {
        let response: OwnerItems<DiagnosticStatus> =
            self.get("/api/dais/owner/diagnostics").await?;
        Ok(response.items)
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

    pub async fn watches(&self) -> ClientResult<OwnerSources> {
        self.get("/api/dais/owner/watches").await
    }

    pub async fn add_watch(&self, watch: &OwnerWatchAdd) -> ClientResult<OwnerSourceAddResult> {
        self.post("/api/dais/owner/watches", watch).await
    }

    pub async fn remove_watch(&self, id: &str) -> ClientResult<OwnerActionResult> {
        self.delete(&format!("/api/dais/owner/watches/{id}")).await
    }

    pub async fn refresh_watches(
        &self,
        id: Option<&str>,
    ) -> ClientResult<OwnerSourceRefreshResult> {
        self.post(
            "/api/dais/owner/watches/refresh",
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

    pub async fn moderation_replies(&self) -> ClientResult<Vec<ModerationReplyRow>> {
        let response: OwnerItems<ModerationReplyRow> =
            self.get("/api/dais/owner/moderation/replies").await?;
        Ok(response.items)
    }

    pub async fn set_reply_moderation_status(
        &self,
        reply_id: &str,
        status: &str,
    ) -> ClientResult<ModerationReplyRow> {
        self.post(
            "/api/dais/owner/moderation/replies/status",
            &ModerationReplyStatus { reply_id, status },
        )
        .await
    }

    pub async fn update_moderation_settings(
        &self,
        settings: &ModerationSettingsUpdate,
    ) -> ClientResult<ModerationState> {
        self.post("/api/dais/owner/moderation/settings", settings)
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
pub struct OwnerSettingsUpdate {
    pub default_visibility: Visibility,
    pub default_protocol: ProtocolRoute,
    pub require_authorized_fetch: bool,
    pub manually_approves_followers: bool,
    pub closed_network: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComposeDraft {
    pub text: String,
    pub visibility: Visibility,
    pub protocol: ProtocolRoute,
    pub encrypt: bool,
    pub in_reply_to: Option<String>,
    pub audience_list_id: Option<String>,
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
pub struct OwnerSavedPost {
    pub id: String,
    pub post_id: Option<String>,
    pub object_id: Option<String>,
    pub canonical_url: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub source: String,
    pub saved_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSavePost {
    pub post_id: Option<String>,
    pub object_id: Option<String>,
    pub canonical_url: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub source: Option<String>,
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
pub struct OwnerDeletedPost {
    pub ok: bool,
    pub id: String,
    pub deleted: bool,
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
    pub description: Option<String>,
    pub access: Option<String>,
    pub expires_in_seconds: Option<u64>,
    pub require_authorized_fetch: Option<bool>,
    pub data_base64: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerMedia {
    pub url: String,
    pub media_type: Option<String>,
    pub description: Option<String>,
    pub access: Option<String>,
    pub authorized_fetch: Option<bool>,
    pub expires_at: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OwnerMediaRevoke<'a> {
    url: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FollowTarget<'a> {
    target: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NotificationRead<'a> {
    id: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OwnerEmptyBody {}

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
pub struct OwnerAudienceList {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub allowed_categories: Vec<String>,
    #[serde(default = "default_audience_group_type")]
    pub group_type: String,
    #[serde(default = "default_audience_membership_visibility")]
    pub membership_visibility: String,
    #[serde(default = "default_audience_posting_policy")]
    pub posting_policy: String,
    #[serde(default)]
    pub purpose_label: String,
    #[serde(default)]
    pub membership_label: String,
    #[serde(default)]
    pub member_actor_ids: Vec<String>,
    #[serde(default)]
    pub member_count: u64,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerAudienceListUpsert {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_audience_group_type")]
    pub group_type: String,
    #[serde(default = "default_audience_membership_visibility")]
    pub membership_visibility: String,
    #[serde(default = "default_audience_posting_policy")]
    pub posting_policy: String,
    #[serde(default)]
    pub allowed_categories: Vec<String>,
    #[serde(default)]
    pub member_actor_ids: Vec<String>,
}

fn default_audience_group_type() -> String {
    "audience".to_string()
}

fn default_audience_membership_visibility() -> String {
    "private".to_string()
}

fn default_audience_posting_policy() -> String {
    "owner".to_string()
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
    pub actor_type: Option<String>,
    pub inbox: String,
    pub shared_inbox: Option<String>,
    pub preferred_username: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub icon_url: Option<String>,
    pub handle: Option<String>,
    pub following_status: Option<String>,
    pub target_public_post: Option<OwnerDiscoveredPost>,
    #[serde(default)]
    pub recent_public_posts: Vec<OwnerDiscoveredPost>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerDiscoveredPost {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub actor_id: Option<String>,
    pub url: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub content: String,
    pub published: Option<String>,
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
    pub context_post_id: Option<String>,
    pub context_post_content: Option<String>,
    pub context_post_content_html: Option<String>,
    pub context_post_visibility: Option<String>,
    pub context_post_protocol: Option<String>,
    pub context_post_published_at: Option<String>,
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
pub struct OwnerE2eeMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_actor_id: String,
    pub sender_device_id: String,
    pub recipient_actor_id: Option<String>,
    #[serde(default = "default_e2ee_protocol")]
    pub e2ee_protocol: String,
    #[serde(default)]
    pub dais_encrypted_message: serde_json::Value,
    pub encrypted_message: serde_json::Value,
    #[serde(default)]
    pub mls_group_id: Option<String>,
    #[serde(default)]
    pub mls_epoch: Option<u64>,
    pub fallback_content: Option<String>,
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    #[serde(default)]
    pub delivery_ids: Vec<String>,
    #[serde(default)]
    pub delivery_statuses: Vec<OwnerDelivery>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eeMessageSend {
    pub recipient_actor_id: String,
    pub recipient_device_id: Option<String>,
    pub sender_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dais_encrypted_message: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_message: Option<serde_json::Value>,
    pub fallback_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<serde_json::Value>,
}

fn default_e2ee_protocol() -> String {
    "dais-mls-v1".to_string()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eeMessageSendResult {
    pub ok: bool,
    pub message: OwnerE2eeMessage,
    pub delivery_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eeDevice {
    pub id: String,
    pub actor_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub protocol: String,
    pub credential: String,
    pub key_package: String,
    pub fingerprint: String,
    pub status: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eeDeviceUpsert {
    pub device_id: String,
    pub display_name: Option<String>,
    pub protocol: String,
    pub credential: String,
    pub key_package: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eeDeviceRef {
    pub device_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eePeerDevice {
    pub id: String,
    pub actor_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub protocol: String,
    pub credential: String,
    pub key_package: String,
    pub fingerprint: String,
    pub trust_state: String,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub trusted_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eePeerDiscoverRequest {
    pub actor_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eePeerDiscoverResult {
    pub actor_id: String,
    #[serde(default)]
    pub items: Vec<OwnerE2eePeerDevice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eePeerTrustRequest {
    pub actor_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub protocol: String,
    pub credential: String,
    pub key_package: String,
    pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerE2eePeerDeviceRef {
    pub actor_id: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchResult {
    #[serde(default)]
    pub posts: Vec<OwnerSearchPost>,
    #[serde(default)]
    pub users: Vec<OwnerSearchUser>,
    #[serde(default)]
    pub sources: Vec<SourceSubscription>,
    #[serde(default)]
    pub source_items: Vec<OwnerSearchSourceItem>,
    #[serde(default)]
    pub public_posts: Vec<OwnerPublicSearchPost>,
    #[serde(default)]
    pub public_actors: Vec<OwnerPublicSearchActor>,
    #[serde(default)]
    pub provider_errors: Vec<OwnerSearchProviderError>,
    #[serde(default)]
    pub public_search_guard: OwnerPublicSearchGuard,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OwnerSearchQuery {
    pub query: String,
    pub scope: String,
    pub confirm_public_sensitive: bool,
    pub provider: Option<String>,
    pub result_type: Option<String>,
    pub servers: Vec<String>,
    pub sort: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub author: Option<String>,
    pub mentions: Option<String>,
    pub lang: Option<String>,
    pub domain: Option<String>,
    pub url: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerPublicSearchGuard {
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub categories: Vec<String>,
    pub message: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchSourceItem {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub canonical_url: Option<String>,
    pub excerpt: Option<String>,
    pub published_at: Option<String>,
    pub read: serde_json::Value,
    pub rights_policy_json: String,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerPublicSearchPost {
    pub provider: String,
    pub network: String,
    pub id: String,
    pub url: String,
    pub content: String,
    pub canonical_url: Option<String>,
    pub actor_id: Option<String>,
    pub actor_handle: Option<String>,
    pub actor_display_name: Option<String>,
    pub content_html: Option<String>,
    pub summary: Option<String>,
    pub object_type: Option<String>,
    pub published_at: Option<String>,
    pub watch_type: Option<String>,
    pub watch_target: Option<String>,
    pub reply_target: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    pub cid: Option<String>,
    pub reply_count: Option<u64>,
    pub repost_count: Option<u64>,
    pub like_count: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerPublicSearchActor {
    pub provider: String,
    pub network: String,
    pub id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub avatar_url: Option<String>,
    pub watch_type: Option<String>,
    pub watch_target: Option<String>,
    pub follow_target: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnerSearchProviderError {
    pub provider: String,
    pub network: String,
    pub error: String,
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
pub struct OwnerWatchAdd {
    pub watch_type: String,
    pub target: String,
    pub title: Option<String>,
    pub cadence_minutes: Option<u16>,
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
    pub require_authorized_fetch: bool,
    #[serde(default)]
    pub manually_approves_followers: bool,
    #[serde(default)]
    pub reply_policy: String,
    #[serde(default)]
    pub ai_enabled: bool,
    #[serde(default)]
    pub ai_model: Option<String>,
    #[serde(default)]
    pub ai_daily_budget: u64,
    #[serde(default)]
    pub reply_queue_count: u64,
    #[serde(default)]
    pub flagged_reply_count: u64,
    #[serde(default)]
    pub hidden_reply_count: u64,
    #[serde(default)]
    pub rejected_reply_count: u64,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModerationReplyRow {
    pub id: String,
    pub post_id: String,
    pub actor_id: String,
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub content: String,
    pub published_at: Option<String>,
    pub created_at: Option<String>,
    pub moderation_status: Option<String>,
    pub moderation_score: Option<f64>,
    #[serde(default)]
    pub moderation_flags: Vec<String>,
    pub moderation_checked_at: Option<String>,
    pub hidden: serde_json::Value,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ModerationReplyStatus<'a> {
    reply_id: &'a str,
    status: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModerationSettingsUpdate {
    pub reply_policy: String,
    pub ai_enabled: bool,
    pub ai_model: Option<String>,
    pub ai_daily_budget: u64,
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
    #[serde(default)]
    pub saved_posts: Vec<OwnerSavedPost>,
    pub followers: Vec<OwnerFollower>,
    pub friends: Vec<OwnerFriend>,
    pub following: Vec<OwnerFollowing>,
    pub audience_lists: Vec<OwnerAudienceList>,
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

fn encode_query(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", url_encode(key), url_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
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
            audience_list_id: None,
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
            audience_list_id: None,
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
    fn owner_e2ee_message_deserializes_mls_metadata() {
        let message: OwnerE2eeMessage = serde_json::from_value(serde_json::json!({
            "id": "msg-1",
            "conversation_id": "conversation-1",
            "sender_actor_id": "https://social.dais.social/users/social",
            "sender_device_id": "mac",
            "recipient_actor_id": "https://social.skpt.cl/users/social",
            "e2ee_protocol": "mls-rfc9420",
            "dais_encrypted_message": {
                "v": 2,
                "protocol": "mls-rfc9420",
                "groupId": "group",
                "epoch": 2,
                "senderDeviceId": "mac",
                "ciphertext": "Y2lwaGVydGV4dA=="
            },
            "encrypted_message": null,
            "mls_group_id": "group",
            "mls_epoch": 2,
            "fallback_content": "Encrypted message",
            "created_at": "today"
        }))
        .unwrap();

        assert_eq!(message.e2ee_protocol, "mls-rfc9420");
        assert_eq!(message.mls_group_id.as_deref(), Some("group"));
        assert_eq!(message.mls_epoch, Some(2));
        assert_eq!(message.encrypted_message, serde_json::Value::Null);
        assert_eq!(message.dais_encrypted_message["v"], 2);
    }

    #[test]
    fn owner_e2ee_send_serializes_only_selected_envelope() {
        let mls = OwnerE2eeMessageSend {
            recipient_actor_id: "https://social.skpt.cl/users/social".into(),
            recipient_device_id: Some("phone".into()),
            sender_device_id: "mac".into(),
            dais_encrypted_message: Some(serde_json::json!({"v": 2})),
            encrypted_message: None,
            fallback_content: Some("Encrypted".into()),
            attachments: Vec::new(),
        };
        let value = serde_json::to_value(mls).unwrap();

        assert!(value.get("dais_encrypted_message").is_some());
        assert!(value.get("encrypted_message").is_none());
    }

    #[test]
    fn snapshot_serializes_for_owner_clients() {
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
            saved_posts: Vec::new(),
            followers: Vec::new(),
            friends: Vec::new(),
            following: Vec::new(),
            audience_lists: Vec::new(),
            sources: Vec::new(),
            moderation: ModerationState {
                closed_network: false,
                block_count: 0,
                allowlist_count: 0,
                require_authorized_fetch: false,
                manually_approves_followers: false,
                reply_policy: "review".to_string(),
                ai_enabled: false,
                ai_model: None,
                ai_daily_budget: 0,
                reply_queue_count: 0,
                flagged_reply_count: 0,
                hidden_reply_count: 0,
                rejected_reply_count: 0,
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
