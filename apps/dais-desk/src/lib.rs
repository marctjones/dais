use dais_client_core::{
    ComposeDraft, DiagnosticStatus, ModerationReplyRow, ModerationState, OwnerApiClient,
    OwnerAudienceList, OwnerCreatedPost, OwnerDeletedPost, OwnerDelivery, OwnerDirectMessage,
    OwnerDiscoveredActor, OwnerFollowResult, OwnerFollower, OwnerFollowing, OwnerFriend,
    OwnerInteraction, OwnerInteractionResult, OwnerNotification, OwnerPost, OwnerProfile,
    OwnerPublicSearchActor, OwnerPublicSearchPost, OwnerSearchQuery, OwnerSearchResult,
    OwnerSection, OwnerSettings, OwnerSourceAddResult, OwnerSourceRefreshResult, OwnerSources,
    OwnerStats, OwnerTimelinePost, OwnerWatchAdd, ProtocolRoute, SourceItem, SourceSubscription,
    Visibility,
};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

slint::include_modules!();

const DEFAULT_INSTANCE_URL: &str = "https://social.dais.social";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredOwnerSettings {
    #[serde(default = "default_instance_url")]
    pub instance_url: String,
    #[serde(default)]
    pub owner_token: Option<String>,
    #[serde(default)]
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub accounts: Vec<StoredOwnerAccount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredOwnerAccount {
    pub id: String,
    pub label: String,
    pub instance_url: String,
    pub owner_token: Option<String>,
}

impl Default for StoredOwnerSettings {
    fn default() -> Self {
        let account = StoredOwnerAccount {
            id: account_id_for(DEFAULT_INSTANCE_URL, &[]),
            label: "Dais Social".to_string(),
            instance_url: DEFAULT_INSTANCE_URL.to_string(),
            owner_token: None,
        };
        Self {
            instance_url: account.instance_url.clone(),
            owner_token: account.owner_token.clone(),
            active_account_id: Some(account.id.clone()),
            accounts: vec![account],
        }
    }
}

#[derive(Clone, Debug)]
pub struct OwnerAccountSummary {
    pub id: String,
    pub label: String,
    pub instance_url: String,
    pub active: bool,
    pub owner_token_present: bool,
}

#[derive(Clone, Debug)]
pub struct DeskData {
    pub snapshot: OwnerSnapshotBundle,
    pub notifications: Vec<OwnerNotification>,
    pub deliveries: Vec<OwnerDelivery>,
    pub direct_messages: Vec<OwnerDirectMessage>,
    pub sources: OwnerSources,
    pub watches: OwnerSources,
    pub moderation_replies: Vec<ModerationReplyRow>,
    pub stats: OwnerStats,
    pub search: OwnerSearchResult,
    pub discovered_actor: Option<OwnerDiscoveredActor>,
    pub api_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OwnerSnapshotBundle {
    pub settings: OwnerSettings,
    pub active_section: OwnerSection,
    pub profile: OwnerProfile,
    pub home_timeline: Vec<OwnerTimelinePost>,
    pub posts: Vec<OwnerPost>,
    pub followers: Vec<OwnerFollower>,
    pub friends: Vec<OwnerFriend>,
    pub following: Vec<OwnerFollowing>,
    pub audience_lists: Vec<OwnerAudienceList>,
    pub sources: Vec<SourceItem>,
    pub moderation: ModerationState,
    pub diagnostics: Vec<DiagnosticStatus>,
}

impl From<dais_client_core::OwnerSnapshot> for OwnerSnapshotBundle {
    fn from(snapshot: dais_client_core::OwnerSnapshot) -> Self {
        Self {
            settings: snapshot.settings,
            active_section: snapshot.active_section,
            profile: snapshot.profile,
            home_timeline: snapshot.home_timeline,
            posts: snapshot.posts,
            followers: snapshot.followers,
            friends: snapshot.friends,
            following: snapshot.following,
            audience_lists: snapshot.audience_lists,
            sources: snapshot.sources,
            moderation: snapshot.moderation,
            diagnostics: snapshot.diagnostics,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComposeState {
    pub text: String,
    pub visibility: Visibility,
    pub protocol: ProtocolRoute,
    pub encrypt: bool,
    pub in_reply_to: Option<String>,
    pub audience_list_id: Option<String>,
    pub recipients: String,
    pub media_description: String,
    pub attachments: Vec<String>,
}

impl Default for ComposeState {
    fn default() -> Self {
        Self {
            text: String::new(),
            visibility: Visibility::Followers,
            protocol: ProtocolRoute::ActivityPub,
            encrypt: false,
            in_reply_to: None,
            audience_list_id: None,
            recipients: String::new(),
            media_description: String::new(),
            attachments: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UiProjection {
    pub mode_nav: Vec<NavItem>,
    pub screen_nav: Vec<NavItem>,
    pub rows: Vec<UiRow>,
    pub inspector_rows: Vec<UiRow>,
    pub accounts: Vec<AccountRow>,
    pub active_mode: String,
    pub active_screen: String,
    pub selected_row: String,
    pub window_title: String,
    pub window_subtitle: String,
    pub attention_summary: String,
    pub privacy_status: String,
    pub status_message: String,
    pub command_text: String,
    pub compose_text: String,
    pub compose_recipients: String,
    pub compose_media_description: String,
    pub compose_encrypt: bool,
    pub compose_visibility: String,
    pub compose_protocol: String,
    pub compose_warning: String,
    pub compose_can_send: bool,
    pub account_label: String,
    pub account_url: String,
    pub account_token: String,
}

pub struct DeskController {
    settings_path: PathBuf,
    settings: StoredOwnerSettings,
    runtime: tokio::runtime::Runtime,
    data: DeskData,
    active_mode: String,
    active_screen: String,
    selected_row: String,
    command_text: String,
    compose: ComposeState,
    status_message: String,
    account_form_label: String,
    account_form_url: String,
    account_form_token: String,
}

impl DeskController {
    pub fn load_default() -> Result<Self, String> {
        Self::new(default_settings_path())
    }

    pub fn new(settings_path: PathBuf) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let settings = load_settings_from(&settings_path)?;
        let active = active_account(&settings).cloned();
        let (account_form_label, account_form_url, account_form_token) = active
            .map(|account| {
                (
                    account.label,
                    account.instance_url,
                    account.owner_token.unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let mut controller = Self {
            settings_path,
            settings,
            runtime,
            data: fixture_data(None),
            active_mode: "home".to_string(),
            active_screen: "today".to_string(),
            selected_row: String::new(),
            command_text: String::new(),
            compose: ComposeState::default(),
            status_message: "Ready.".to_string(),
            account_form_label,
            account_form_url,
            account_form_token,
        };
        controller.refresh();
        Ok(controller)
    }

    pub fn fixture_for_tests() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let settings = StoredOwnerSettings::default();
        Self {
            settings_path: PathBuf::from("fixture-owner-settings.json"),
            settings,
            runtime,
            data: fixture_data(None),
            active_mode: "home".to_string(),
            active_screen: "today".to_string(),
            selected_row: "post:fixture-private-post".to_string(),
            command_text: String::new(),
            compose: ComposeState::default(),
            status_message: "Fixture mode.".to_string(),
            account_form_label: "Dais Social".to_string(),
            account_form_url: DEFAULT_INSTANCE_URL.to_string(),
            account_form_token: String::new(),
        }
    }

    pub fn refresh(&mut self) {
        let settings = normalize_settings(self.settings.clone());
        self.settings = settings.clone();
        self.data = match self.fetch_live_data(&settings) {
            Ok(data) => {
                self.status_message = "Loaded owner server state.".to_string();
                data
            }
            Err(error) => {
                self.status_message = format!("Showing local preview data: {error}");
                fixture_data(Some(error))
            }
        };
        if self.selected_row.is_empty() {
            self.selected_row = self.first_row_id();
        }
    }

    pub fn select_mode(&mut self, mode: &str) {
        self.active_mode = mode.to_string();
        self.active_screen = match mode {
            "people" => "find".to_string(),
            "server" => "health".to_string(),
            _ => "today".to_string(),
        };
        self.selected_row = self.first_row_id();
    }

    pub fn select_screen(&mut self, screen: &str) {
        self.active_screen = screen.to_string();
        self.active_mode = mode_for_screen(screen).to_string();
        if screen == "accounts" {
            if let Some(account) = active_account(&self.settings) {
                self.account_form_label = account.label.clone();
                self.account_form_url = account.instance_url.clone();
                self.account_form_token = account.owner_token.clone().unwrap_or_default();
            }
        }
        self.selected_row = self.first_row_id();
    }

    pub fn select_row(&mut self, row_id: &str) {
        self.selected_row = row_id.to_string();
        if let Some(object_id) = row_id.strip_prefix("post:") {
            self.compose.in_reply_to = None;
            self.status_message = format!("Selected post context {object_id}.");
        } else if let Some(object_id) = row_id.strip_prefix("timeline:") {
            self.status_message = format!("Selected timeline item {object_id}.");
        } else if let Some(actor) = row_id.strip_prefix("actor:") {
            self.status_message = format!("Selected relationship context for {actor}.");
        }
    }

    pub fn run_command(&mut self, command: &str) {
        let query = command.trim();
        self.command_text = query.to_string();
        if query.is_empty() {
            self.status_message =
                "Enter a handle, URL, feed, domain, command, or search text.".into();
            return;
        }
        self.active_mode = "people".to_string();
        self.active_screen = "find".to_string();
        match self.search_or_discover(query) {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("Search failed: {error}"),
        }
        self.selected_row = self.first_row_id();
    }

    pub fn row_action(&mut self, row_id: &str, action: &str) {
        if action.trim().is_empty() {
            return;
        }
        let result = match action {
            "Reply" => self.prepare_reply(row_id),
            "Favorite" => self.interact(row_id, "favorite"),
            "Boost" | "Repost" => self.interact(row_id, "boost"),
            "Delete" => self.delete_post(row_id),
            "Mark read" => self.mark_notification_read(row_id),
            "Approve" => self.set_follower_status(row_id, "approved"),
            "Reject" => self.set_follower_status(row_id, "rejected"),
            "Remove" => self.set_follower_status(row_id, "removed"),
            "Follow" => self.follow(row_id),
            "Unfollow" | "Cancel" => self.unfollow(row_id),
            "Watch" => self.watch(row_id),
            "Stop watching" => self.remove_source_or_watch(row_id),
            "Refresh" => self.refresh_source_or_watch(row_id),
            "Approve reply" => self.set_reply_status(row_id, "approved"),
            "Hide reply" => self.set_reply_status(row_id, "hidden"),
            "Reject reply" => self.set_reply_status(row_id, "rejected"),
            "Block" => self.block(row_id),
            "Unblock" => self.unblock(row_id),
            "Open original" | "Open link" => self.open_external(row_id),
            "Open context" => {
                self.selected_row = related_context(row_id).unwrap_or(row_id).to_string();
                Ok("Opened related context.".to_string())
            }
            "Inspect delivery" => {
                self.active_mode = "server".to_string();
                self.active_screen = "deliveries".to_string();
                self.selected_row = row_id.to_string();
                Ok("Opened delivery inspector.".to_string())
            }
            "Copy evidence" => Ok("Evidence is available in Diagnostics details.".to_string()),
            _ => Ok(format!(
                "{action} is visible but not destructive in preview mode."
            )),
        };
        match result {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("{action} failed: {error}"),
        }
        if matches!(
            action,
            "Favorite"
                | "Boost"
                | "Repost"
                | "Delete"
                | "Mark read"
                | "Approve"
                | "Reject"
                | "Remove"
                | "Follow"
                | "Unfollow"
                | "Cancel"
                | "Stop watching"
                | "Refresh"
                | "Approve reply"
                | "Hide reply"
                | "Reject reply"
                | "Block"
                | "Unblock"
        ) {
            self.refresh();
        }
    }

    pub fn save_account(&mut self, label: &str, instance_url: &str, owner_token: &str) {
        self.account_form_label = label.trim().to_string();
        self.account_form_url = instance_url.trim().to_string();
        self.account_form_token = owner_token.to_string();
        let result = self.save_account_inner(label, instance_url, owner_token);
        match result {
            Ok(()) => {
                self.status_message = "Saved account and switched active owner API target.".into();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Save account failed: {error}"),
        }
    }

    pub fn switch_account(&mut self, account_id: &str) {
        if !self
            .settings
            .accounts
            .iter()
            .any(|account| account.id == account_id)
        {
            self.status_message = "Account not found.".into();
            return;
        }
        self.settings.active_account_id = Some(account_id.to_string());
        match persist_settings_to(
            &self.settings_path,
            normalize_settings(self.settings.clone()),
        ) {
            Ok(()) => {
                self.settings = load_settings_from(&self.settings_path).unwrap_or_default();
                self.status_message =
                    "Switched account. Reads, posts, follows, watches, and server commands use it now."
                        .into();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Switch failed: {error}"),
        }
    }

    pub fn delete_account(&mut self, account_id: &str) {
        if self.settings.accounts.len() <= 1 {
            self.status_message = "At least one account profile is required.".into();
            return;
        }
        self.settings
            .accounts
            .retain(|account| account.id != account_id);
        if self.settings.active_account_id.as_deref() == Some(account_id) {
            self.settings.active_account_id = self.settings.accounts.first().map(|a| a.id.clone());
        }
        match persist_settings_to(
            &self.settings_path,
            normalize_settings(self.settings.clone()),
        ) {
            Ok(()) => {
                self.settings = load_settings_from(&self.settings_path).unwrap_or_default();
                self.status_message = "Deleted account profile.".into();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Delete account failed: {error}"),
        }
    }

    pub fn compose_set_visibility(&mut self, value: &str) {
        self.compose.visibility = match value {
            "public" => Visibility::Public,
            "direct" => Visibility::Direct,
            "unlisted" => Visibility::Unlisted,
            _ => Visibility::Followers,
        };
        self.active_mode = "home".into();
        self.active_screen = "compose".into();
    }

    pub fn compose_set_protocol(&mut self, value: &str) {
        self.compose.protocol = match value {
            "both" => ProtocolRoute::Both,
            "bluesky" | "atproto" => ProtocolRoute::AtProto,
            _ => ProtocolRoute::ActivityPub,
        };
        self.active_mode = "home".into();
        self.active_screen = "compose".into();
    }

    pub fn update_compose_from_ui(
        &mut self,
        text: &str,
        recipients: &str,
        media_description: &str,
        encrypt: bool,
    ) {
        self.compose.text = text.to_string();
        self.compose.recipients = recipients.to_string();
        self.compose.media_description = media_description.to_string();
        self.compose.encrypt = encrypt;
    }

    pub fn compose_send(&mut self) {
        let result = self.compose_send_inner();
        match result {
            Ok(message) => {
                self.status_message = message;
                self.compose.text.clear();
                self.compose.recipients.clear();
                self.compose.media_description.clear();
                self.compose.in_reply_to = None;
                self.active_screen = "today".into();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Post failed: {error}"),
        }
    }

    pub fn projection(&self) -> UiProjection {
        let rows = self.rows_for_active_screen();
        let selected_row = if rows.iter().any(|row| row.id.as_str() == self.selected_row) {
            self.selected_row.clone()
        } else {
            rows.first()
                .map(|row| row.id.to_string())
                .unwrap_or_default()
        };
        let inspector_rows = self.inspector_rows(&selected_row);
        let unread = self
            .data
            .notifications
            .iter()
            .filter(|notice| !json_truthy(&notice.read))
            .count();
        let failed = self
            .data
            .deliveries
            .iter()
            .filter(|delivery| delivery.status == "failed")
            .count();
        let attention =
            if unread == 0 && failed == 0 && self.data.snapshot.moderation.reply_queue_count() == 0
            {
                "All clear".to_string()
            } else {
                format!(
                    "{} unread, {} failed, {} review",
                    unread,
                    failed,
                    self.data.snapshot.moderation.reply_queue_count()
                )
            };
        let compose_warning = compose_warning(&self.compose);
        let account = active_account(&self.settings);
        UiProjection {
            mode_nav: self.mode_nav(unread, failed),
            screen_nav: self.screen_nav(),
            rows,
            inspector_rows,
            accounts: account_summaries(&self.settings)
                .into_iter()
                .map(account_row)
                .collect(),
            active_mode: self.active_mode.clone(),
            active_screen: self.active_screen.clone(),
            selected_row,
            window_title: self.title_for_active_screen(),
            window_subtitle: self.subtitle_for_active_screen(),
            attention_summary: attention,
            privacy_status: format!(
                "Default audience: {}. Graph and watches are owner-only.",
                visibility_label(&self.data.snapshot.settings.default_visibility)
            ),
            status_message: self.status_message.clone(),
            command_text: self.command_text.clone(),
            compose_text: self.compose.text.clone(),
            compose_recipients: self.compose.recipients.clone(),
            compose_media_description: self.compose.media_description.clone(),
            compose_encrypt: self.compose.encrypt,
            compose_visibility: visibility_label(&self.compose.visibility).to_lowercase(),
            compose_protocol: protocol_label(&self.compose.protocol).to_lowercase(),
            compose_can_send: compose_can_send(&self.compose),
            compose_warning,
            account_label: self
                .account_form_label
                .clone()
                .if_empty_else(|| account.map(|a| a.label.clone()).unwrap_or_default()),
            account_url: self
                .account_form_url
                .clone()
                .if_empty_else(|| account.map(|a| a.instance_url.clone()).unwrap_or_default()),
            account_token: self.account_form_token.clone(),
        }
    }

    fn fetch_live_data(&self, settings: &StoredOwnerSettings) -> Result<DeskData, String> {
        let token = settings
            .owner_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "owner token is required".to_string())?;
        let client = OwnerApiClient::new(&settings.instance_url, token);
        self.runtime.block_on(async move {
            let snapshot = client.snapshot().await.map_err(|error| error.to_string())?;
            let notifications = client.notifications().await.unwrap_or_default();
            let deliveries = client.deliveries().await.unwrap_or_default();
            let direct_messages = client.direct_messages().await.unwrap_or_default();
            let sources = client.sources().await.unwrap_or_else(|_| OwnerSources {
                subscriptions: Vec::new(),
                items: Vec::new(),
            });
            let watches = client.watches().await.unwrap_or_else(|_| OwnerSources {
                subscriptions: Vec::new(),
                items: Vec::new(),
            });
            let moderation_replies = client.moderation_replies().await.unwrap_or_default();
            let stats = client.stats().await.unwrap_or_default();
            Ok(DeskData {
                snapshot: snapshot.into(),
                notifications,
                deliveries,
                direct_messages,
                sources,
                watches,
                moderation_replies,
                stats,
                search: OwnerSearchResult::default(),
                discovered_actor: None,
                api_error: None,
            })
        })
    }

    fn client(&self) -> Result<OwnerApiClient, String> {
        let token = self
            .settings
            .owner_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "owner token is required")?;
        Ok(OwnerApiClient::new(&self.settings.instance_url, token))
    }

    fn search_or_discover(&mut self, query: &str) -> Result<String, String> {
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            self.data.search = fixture_search(query);
            return Ok(
                "Preview search results are shown. Add an owner token for live search.".into(),
            );
        }
        let client = self.client()?;
        let query_string = query.to_string();
        let is_lookup = looks_like_handle_or_url(query);
        let result = self.runtime.block_on(async move {
            if is_lookup {
                let discovered = client.discover_actor(&query_string).await.ok();
                let search = client
                    .search_with_options(&OwnerSearchQuery {
                        query: query_string,
                        scope: "public".to_string(),
                        confirm_public_sensitive: false,
                        ..OwnerSearchQuery::default()
                    })
                    .await
                    .unwrap_or_default();
                Ok::<_, String>((discovered, search))
            } else {
                let search = client
                    .search_with_options(&OwnerSearchQuery {
                        query: query_string,
                        scope: "public".to_string(),
                        confirm_public_sensitive: false,
                        ..OwnerSearchQuery::default()
                    })
                    .await
                    .map_err(|error| error.to_string())?;
                Ok((None, search))
            }
        })?;
        self.data.discovered_actor = result.0;
        self.data.search = result.1;
        Ok(
            "Loaded discovery results. Follow sends a request; Watch reads public posts privately."
                .into(),
        )
    }

    fn prepare_reply(&mut self, row_id: &str) -> Result<String, String> {
        let object_id = object_id_from_row(row_id).ok_or_else(|| "no post context".to_string())?;
        self.compose.in_reply_to = Some(object_id.to_string());
        self.compose.visibility = Visibility::Followers;
        self.compose.protocol = ProtocolRoute::ActivityPub;
        self.active_mode = "home".to_string();
        self.active_screen = "compose".to_string();
        Ok("Reply context attached. Audience is still visible before sending.".into())
    }

    fn interact(&self, row_id: &str, interaction: &str) -> Result<String, String> {
        let object_id =
            object_id_from_row(row_id).ok_or_else(|| "no post object id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(format!("Preview {interaction} recorded for {object_id}."));
        }
        let client = self.client()?;
        let result: OwnerInteractionResult = self.runtime.block_on(async move {
            client
                .interact(&OwnerInteraction {
                    object_id: object_id.to_string(),
                    interaction: interaction.to_string(),
                })
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "{} sent; {} delivery records queued.",
            result.interaction,
            result.delivery_ids.len()
        ))
    }

    fn delete_post(&self, row_id: &str) -> Result<String, String> {
        let object_id =
            object_id_from_row(row_id).ok_or_else(|| "no post object id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(
                "Preview delete: this would remove the selected post and queue deletes.".into(),
            );
        }
        let client = self.client()?;
        let result: OwnerDeletedPost = self.runtime.block_on(async move {
            client
                .delete_post(object_id)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Deleted {}; {} delivery deletes queued.",
            result.id,
            result.delivery_ids.len()
        ))
    }

    fn mark_notification_read(&self, row_id: &str) -> Result<String, String> {
        let id = row_id
            .strip_prefix("notification:")
            .ok_or_else(|| "no notification id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview notification marked read.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .mark_notification_read(id)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Notification marked read.".into())
    }

    fn set_follower_status(&self, row_id: &str, status: &str) -> Result<String, String> {
        let actor = row_id
            .strip_prefix("follower:")
            .ok_or_else(|| "no follower actor id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(format!("Preview follower status changed to {status}."));
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .set_follower_status(actor, status)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!("Follower status changed to {status}."))
    }

    fn follow(&self, row_id: &str) -> Result<String, String> {
        let target = target_from_row(row_id).ok_or_else(|| "no follow target".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview follow request: remote account may be notified.".into());
        }
        let client = self.client()?;
        let result: OwnerFollowResult = self.runtime.block_on(async move {
            client
                .follow_actor(target)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Follow request is {}; {} deliveries queued.",
            result.following.status,
            result.delivery_ids.len()
        ))
    }

    fn unfollow(&self, row_id: &str) -> Result<String, String> {
        let target = target_from_row(row_id).ok_or_else(|| "no follow target".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview unfollow/cancel action recorded.".into());
        }
        let client = self.client()?;
        let result: OwnerFollowResult = self.runtime.block_on(async move {
            client
                .unfollow_actor(target)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Unfollow/cancel sent; {} deliveries queued.",
            result.delivery_ids.len()
        ))
    }

    fn watch(&self, row_id: &str) -> Result<String, String> {
        let target = target_from_row(row_id).ok_or_else(|| "no watch target".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview watch added. No follow request or remote notice is sent.".into());
        }
        let client = self.client()?;
        let result: OwnerSourceAddResult = self.runtime.block_on(async move {
            client
                .add_watch(&OwnerWatchAdd {
                    watch_type: "activitypub".to_string(),
                    target: target.to_string(),
                    title: None,
                    cadence_minutes: Some(60),
                    private_reader_only: true,
                    excerpt_only: true,
                    link_required: true,
                    attribution_required: true,
                    image_allowed: false,
                    full_text_allowed: false,
                })
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Watching {} privately. Only public posts will be fetched.",
            result.source.url
        ))
    }

    fn remove_source_or_watch(&self, row_id: &str) -> Result<String, String> {
        let id = row_id
            .strip_prefix("watch:")
            .or_else(|| row_id.strip_prefix("source:"))
            .ok_or_else(|| "no source/watch id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview source/watch removed.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            if row_id.starts_with("watch:") {
                client.remove_watch(id).await
            } else {
                client.remove_source(id).await
            }
            .map_err(|error| error.to_string())
        })?;
        Ok("Source/watch removed.".into())
    }

    fn refresh_source_or_watch(&self, row_id: &str) -> Result<String, String> {
        let id = row_id
            .strip_prefix("watch:")
            .or_else(|| row_id.strip_prefix("source:"))
            .ok_or_else(|| "no source/watch id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview refresh completed.".into());
        }
        let client = self.client()?;
        let result: OwnerSourceRefreshResult = self.runtime.block_on(async move {
            if row_id.starts_with("watch:") {
                client.refresh_watches(Some(id)).await
            } else {
                client.refresh_sources(Some(id)).await
            }
            .map_err(|error| error.to_string())
        })?;
        Ok(format!("Refresh checked {} source(s).", result.items.len()))
    }

    fn set_reply_status(&self, row_id: &str, status: &str) -> Result<String, String> {
        let id = row_id
            .strip_prefix("moderation-reply:")
            .ok_or_else(|| "no reply id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(format!("Preview moderation status changed to {status}."));
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .set_reply_moderation_status(id, status)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!("Reply moderation status changed to {status}."))
    }

    fn block(&self, row_id: &str) -> Result<String, String> {
        let target = target_from_row(row_id).ok_or_else(|| "no block target".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview block recorded.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .block_actor(target, Some("Blocked from Dais Desk"))
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Actor blocked.".into())
    }

    fn unblock(&self, row_id: &str) -> Result<String, String> {
        let value = row_id
            .strip_prefix("block:")
            .ok_or_else(|| "no block value".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview unblock recorded.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .unblock(value)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Block removed.".into())
    }

    fn open_external(&self, row_id: &str) -> Result<String, String> {
        let url = self
            .find_row(row_id)
            .and_then(|row| extract_first_url(&row.detail))
            .or_else(|| row_id.strip_prefix("url:").map(ToOwned::to_owned))
            .ok_or_else(|| "no external URL on this item".to_string())?;
        open_url(&url)?;
        Ok(format!("Opened {url} in the default browser."))
    }

    fn compose_send_inner(&mut self) -> Result<String, String> {
        if !compose_can_send(&self.compose) {
            return Err(compose_warning(&self.compose));
        }
        let draft = ComposeDraft {
            text: self.compose.text.trim().to_string(),
            visibility: self.compose.visibility.clone(),
            protocol: self.compose.protocol.clone(),
            encrypt: self.compose.encrypt,
            in_reply_to: self.compose.in_reply_to.clone(),
            audience_list_id: self.compose.audience_list_id.clone(),
            recipients: split_list(&self.compose.recipients),
            attachments: self.compose.attachments.clone(),
        };
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(format!(
                "Preview post prepared for {} via {}.",
                visibility_label(&draft.visibility),
                protocol_label(&draft.protocol)
            ));
        }
        let client = self.client()?;
        let result: OwnerCreatedPost = self.runtime.block_on(async move {
            client
                .create_post(&draft)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Posted {} via {}; {} deliveries queued.",
            result.visibility,
            result.protocol,
            result.delivery_ids.len()
        ))
    }

    fn save_account_inner(
        &mut self,
        label: &str,
        instance_url: &str,
        owner_token: &str,
    ) -> Result<(), String> {
        let mut settings = self.settings.clone();
        let instance_url =
            normalize_instance_url(instance_url).unwrap_or_else(|| settings.instance_url.clone());
        let label = optional_trimmed(label).unwrap_or_else(|| account_label(&instance_url));
        let existing_index = settings
            .accounts
            .iter()
            .position(|account| account.instance_url == instance_url);
        let saved_id = if let Some(index) = existing_index {
            let account = &mut settings.accounts[index];
            account.label = label;
            account.instance_url = instance_url;
            if !owner_token.trim().is_empty() {
                account.owner_token = Some(owner_token.to_string());
            }
            account.id.clone()
        } else {
            let existing_ids: Vec<String> =
                settings.accounts.iter().map(|a| a.id.clone()).collect();
            let account = StoredOwnerAccount {
                id: account_id_for(&instance_url, &existing_ids),
                label,
                instance_url,
                owner_token: (!owner_token.trim().is_empty()).then(|| owner_token.to_string()),
            };
            let id = account.id.clone();
            settings.accounts.push(account);
            id
        };
        settings.active_account_id = Some(saved_id);
        self.settings = normalize_settings(settings);
        persist_settings_to(&self.settings_path, self.settings.clone())
    }

    fn rows_for_active_screen(&self) -> Vec<UiRow> {
        match self.active_screen.as_str() {
            "today" => self.home_today_rows(),
            "inbox" => self.inbox_rows(),
            "compose" => self.compose_context_rows(),
            "posts" => self.post_rows(),
            "saved" => self.saved_rows(),
            "find" => self.find_rows(),
            "relationship" => self.relationship_rows(),
            "friends" => self.friend_rows(),
            "followers" => self.follower_rows(),
            "following" => self.following_rows(),
            "watches" => self.watch_rows(),
            "audience" => self.audience_rows(),
            "blocks" => self.block_rows(),
            "health" => self.health_rows(),
            "deliveries" => self.delivery_rows(),
            "moderation" => self.moderation_rows(),
            "identity" => self.identity_rows(),
            "accounts" => self.account_rows_as_ui(),
            "settings" => self.settings_rows(),
            "stats" => self.stats_rows(),
            _ => self.home_today_rows(),
        }
    }

    fn mode_nav(&self, unread: usize, failed: usize) -> Vec<NavItem> {
        vec![
            nav("home", "Home", unread, self.active_mode == "home"),
            nav(
                "people",
                "People",
                self.data.snapshot.followers.len()
                    + self.data.snapshot.following.len()
                    + self.data.watches.subscriptions.len(),
                self.active_mode == "people",
            ),
            nav("server", "Server", failed, self.active_mode == "server"),
        ]
    }

    fn screen_nav(&self) -> Vec<NavItem> {
        let screens: &[(&str, &str, usize)] = match self.active_mode.as_str() {
            "people" => &[
                ("find", "Find", self.find_rows().len()),
                ("relationship", "Relationship", 0),
                ("friends", "Friends", self.data.snapshot.friends.len()),
                ("followers", "Followers", self.data.snapshot.followers.len()),
                ("following", "Following", self.data.snapshot.following.len()),
                (
                    "watches",
                    "Watches & Sources",
                    self.data.watches.subscriptions.len(),
                ),
                (
                    "audience",
                    "Audience Groups",
                    self.data.snapshot.audience_lists.len(),
                ),
                (
                    "blocks",
                    "Blocks & Mutes",
                    self.data.snapshot.moderation.blocks.len(),
                ),
            ],
            "server" => &[
                ("health", "Health", self.data.snapshot.diagnostics.len()),
                ("deliveries", "Deliveries", self.data.deliveries.len()),
                (
                    "moderation",
                    "Moderation",
                    self.data.moderation_replies.len(),
                ),
                ("identity", "Identity", 0),
                (
                    "accounts",
                    "Accounts & Tokens",
                    self.settings.accounts.len(),
                ),
                ("settings", "Settings", 0),
                ("stats", "Stats", 0),
            ],
            _ => &[
                ("today", "Today", self.home_today_rows().len()),
                ("inbox", "Inbox", self.inbox_rows().len()),
                ("compose", "Compose", 0),
                ("posts", "My Posts", self.data.snapshot.posts.len()),
                ("saved", "Saved & Drafts", 2),
            ],
        };
        screens
            .iter()
            .map(|(id, title, count)| nav(id, title, *count, self.active_screen == *id))
            .collect()
    }

    fn title_for_active_screen(&self) -> String {
        match self.active_screen.as_str() {
            "today" => "Today".into(),
            "inbox" => "Inbox".into(),
            "compose" => "Compose".into(),
            "posts" => "My Posts".into(),
            "saved" => "Saved & Drafts".into(),
            "find" => "Find".into(),
            "relationship" => "Relationship".into(),
            "friends" => "Friends".into(),
            "followers" => "Followers".into(),
            "following" => "Following".into(),
            "watches" => "Watches & Sources".into(),
            "audience" => "Audience Groups".into(),
            "blocks" => "Blocks & Mutes".into(),
            "health" => "Health".into(),
            "deliveries" => "Deliveries".into(),
            "moderation" => "Moderation".into(),
            "identity" => "Identity".into(),
            "accounts" => "Accounts & Tokens".into(),
            "settings" => "Settings".into(),
            "stats" => "Stats".into(),
            _ => "Dais Desk".into(),
        }
    }

    fn subtitle_for_active_screen(&self) -> String {
        match self.active_screen.as_str() {
            "today" => "Read, reply, and handle the day without protocol clutter.".into(),
            "inbox" => {
                "Notifications, DMs, requests, delivery failures, and moderation attention.".into()
            }
            "compose" => "Audience and visibility are selected before posting.".into(),
            "find" => "Handles, URLs, domains, posts, feeds, and public search.".into(),
            "relationship" => "One account, all relationship consequences.".into(),
            "watches" => "Private monitoring of public posts without follow approval.".into(),
            "deliveries" => "Where posts went and what needs operator action.".into(),
            "moderation" => "Review replies, warnings, blocks, and sensitivity policy.".into(),
            "accounts" => "Multiple Dais instances and owner tokens.".into(),
            _ => "Private-by-default social work with operator controls nearby.".into(),
        }
    }

    fn home_today_rows(&self) -> Vec<UiRow> {
        let mut rows = Vec::new();
        for notice in self
            .data
            .notifications
            .iter()
            .filter(|n| !json_truthy(&n.read))
        {
            rows.push(notification_row(notice));
        }
        for dm in &self.data.direct_messages {
            rows.push(dm_row(dm));
        }
        for post in &self.data.snapshot.home_timeline {
            rows.push(timeline_row(post));
        }
        for post in &self.data.snapshot.posts {
            rows.push(post_row(post));
        }
        for delivery in self.data.deliveries.iter().filter(|d| d.status == "failed") {
            rows.push(delivery_attention_row(delivery));
        }
        rows
    }

    fn inbox_rows(&self) -> Vec<UiRow> {
        let mut rows: Vec<UiRow> = self
            .data
            .notifications
            .iter()
            .map(notification_row)
            .collect();
        rows.extend(self.data.direct_messages.iter().map(dm_row));
        rows.extend(
            self.data
                .snapshot
                .followers
                .iter()
                .filter(|f| f.status == "pending")
                .map(follower_row),
        );
        rows.extend(
            self.data
                .moderation_replies
                .iter()
                .map(moderation_reply_row),
        );
        rows.extend(
            self.data
                .deliveries
                .iter()
                .filter(|d| d.status == "failed")
                .map(delivery_attention_row),
        );
        rows
    }

    fn compose_context_rows(&self) -> Vec<UiRow> {
        let mut rows = vec![row(
            "compose:privacy",
            "Audience preview",
            "Private by default",
            &compose_warning(&self.compose),
            visibility_label(&self.compose.visibility),
            "ok",
            "",
            "",
        )];
        if let Some(reply) = &self.compose.in_reply_to {
            rows.push(row(
                &format!("post:{reply}"),
                "Reply context",
                "This reply will keep its own audience choice",
                reply,
                "Reply",
                "info",
                "Open context",
                "",
            ));
        }
        rows
    }

    fn post_rows(&self) -> Vec<UiRow> {
        self.data.snapshot.posts.iter().map(post_row).collect()
    }

    fn saved_rows(&self) -> Vec<UiRow> {
        vec![
            row(
                "saved:owner-only",
                "Saved posts",
                "Owner-only bookmarks",
                "Saved items are local to the owner and are not advertised to followers.",
                "Owner-only",
                "ok",
                "Open context",
                "",
            ),
            row(
                "draft:private",
                "Drafts",
                "Unsent posts",
                "Drafts preserve the intended audience and route before reopening.",
                "Draft",
                "warn",
                "Open context",
                "Delete",
            ),
        ]
    }

    fn find_rows(&self) -> Vec<UiRow> {
        let mut rows = Vec::new();
        if let Some(actor) = &self.data.discovered_actor {
            rows.push(discovered_actor_row(actor));
        }
        rows.extend(self.data.search.public_actors.iter().map(public_actor_row));
        rows.extend(self.data.search.public_posts.iter().map(public_post_row));
        rows.extend(self.data.search.sources.iter().map(source_subscription_row));
        rows.extend(
            self.data
                .search
                .source_items
                .iter()
                .map(search_source_item_row),
        );
        if rows.is_empty() {
            rows.push(row(
                "find:empty",
                "Search the social web",
                "Handles, URLs, RSS/Atom, domains, and public indexes",
                "Paste a handle or URL, or enter a public search. Sensitive-looking public searches ask for confirmation before broad lookup.",
                "Private start",
                "ok",
                "",
                "",
            ));
        }
        rows
    }

    fn relationship_rows(&self) -> Vec<UiRow> {
        if let Some(actor) = &self.data.discovered_actor {
            return vec![discovered_actor_row(actor)];
        }
        let mut rows = Vec::new();
        rows.extend(self.data.snapshot.friends.iter().map(friend_row));
        rows.extend(self.data.snapshot.followers.iter().map(follower_row));
        rows.extend(self.data.snapshot.following.iter().map(following_row));
        rows
    }

    fn friend_rows(&self) -> Vec<UiRow> {
        self.data.snapshot.friends.iter().map(friend_row).collect()
    }

    fn follower_rows(&self) -> Vec<UiRow> {
        self.data
            .snapshot
            .followers
            .iter()
            .map(follower_row)
            .collect()
    }

    fn following_rows(&self) -> Vec<UiRow> {
        self.data
            .snapshot
            .following
            .iter()
            .map(following_row)
            .collect()
    }

    fn watch_rows(&self) -> Vec<UiRow> {
        let mut rows: Vec<UiRow> = self
            .data
            .watches
            .subscriptions
            .iter()
            .map(watch_subscription_row)
            .collect();
        rows.extend(
            self.data
                .sources
                .subscriptions
                .iter()
                .map(source_subscription_row),
        );
        rows.extend(self.data.watches.items.iter().map(source_item_row));
        rows.extend(self.data.sources.items.iter().map(source_item_row));
        rows
    }

    fn audience_rows(&self) -> Vec<UiRow> {
        self.data
            .snapshot
            .audience_lists
            .iter()
            .map(audience_row)
            .collect()
    }

    fn block_rows(&self) -> Vec<UiRow> {
        let mut rows: Vec<UiRow> = self
            .data
            .snapshot
            .moderation
            .blocks
            .iter()
            .map(|block| {
                row(
                    &format!("block:{}", block.actor_id),
                    &block.actor_id,
                    block.blocked_domain.as_deref().unwrap_or("Actor block"),
                    block
                        .reason
                        .as_deref()
                        .unwrap_or("Blocked from seeing or interacting where policy applies."),
                    "Blocked",
                    "danger",
                    "Unblock",
                    "",
                )
            })
            .collect();
        rows.extend(self.data.snapshot.moderation.allowlist.iter().map(|host| {
            row(
                &format!("allow:{}", host.host),
                &host.host,
                "Allowed host",
                host.note
                    .as_deref()
                    .unwrap_or("Allowed to participate in closed-network posture."),
                "Allowed",
                "ok",
                "Remove",
                "",
            )
        }));
        rows
    }

    fn health_rows(&self) -> Vec<UiRow> {
        let mut rows: Vec<UiRow> = self
            .data
            .snapshot
            .diagnostics
            .iter()
            .map(diagnostic_row)
            .collect();
        rows.push(row(
            "health:profile",
            &self.data.snapshot.profile.public_handle,
            "Public profile reachability",
            &format!(
                "Actor URL: {}",
                compact_url(&self.data.snapshot.profile.actor_url)
            ),
            "Identity",
            "info",
            "Open original",
            "",
        ));
        rows
    }

    fn delivery_rows(&self) -> Vec<UiRow> {
        self.data.deliveries.iter().map(delivery_row).collect()
    }

    fn moderation_rows(&self) -> Vec<UiRow> {
        let mut rows = vec![row(
            "moderation:settings",
            "Moderation policy",
            &format!(
                "Reply policy: {}",
                self.data.snapshot.moderation.reply_policy
            ),
            &format!(
                "AI advisory: {}. Daily budget: {}. Manual followers: {}.",
                if self.data.snapshot.moderation.ai_enabled {
                    "on"
                } else {
                    "off"
                },
                self.data.snapshot.moderation.ai_daily_budget,
                self.data.snapshot.moderation.manually_approves_followers
            ),
            "Policy",
            "info",
            "",
            "",
        )];
        rows.extend(
            self.data
                .moderation_replies
                .iter()
                .map(moderation_reply_row),
        );
        rows
    }

    fn identity_rows(&self) -> Vec<UiRow> {
        let profile = &self.data.snapshot.profile;
        vec![
            row(
                "identity:profile",
                profile
                    .display_name
                    .as_deref()
                    .unwrap_or(profile.public_handle.as_str()),
                &profile.public_handle,
                profile
                    .summary
                    .as_deref()
                    .unwrap_or("No profile summary configured."),
                "Public",
                "warn",
                "Open original",
                "",
            ),
            row(
                "identity:actor",
                "Federated identity",
                &profile.actor_type,
                &format!(
                    "Profile is reachable at {}",
                    compact_url(&profile.actor_url)
                ),
                "Actor",
                "info",
                "Copy evidence",
                "",
            ),
        ]
    }

    fn account_rows_as_ui(&self) -> Vec<UiRow> {
        account_summaries(&self.settings)
            .into_iter()
            .map(|account| {
                row(
                    &format!("account:{}", account.id),
                    &account.label,
                    &account.instance_url,
                    if account.owner_token_present {
                        "Owner token is stored for this instance."
                    } else {
                        "No owner token is stored. This account opens in preview mode."
                    },
                    if account.active { "Active" } else { "Account" },
                    if account.owner_token_present {
                        "ok"
                    } else {
                        "warn"
                    },
                    if account.active { "" } else { "Switch" },
                    "Delete",
                )
            })
            .collect()
    }

    fn settings_rows(&self) -> Vec<UiRow> {
        vec![
            row(
                "settings:audience",
                "Default audience",
                visibility_label(&self.data.snapshot.settings.default_visibility),
                "Every compose surface shows this before protocol route so public sharing is deliberate.",
                "Privacy",
                "ok",
                "",
                "",
            ),
            row(
                "settings:route",
                "Default route",
                protocol_label(&self.data.snapshot.settings.default_protocol),
                "Bluesky and public routes are shown explicitly before send.",
                "Route",
                "info",
                "",
                "",
            ),
            row(
                "settings:fetch",
                "Authorized fetch",
                "Private mode",
                if self.data.snapshot.moderation.require_authorized_fetch {
                    "Read endpoints should require authorized fetch for private/followers content."
                } else {
                    "Authorized fetch is not enforced according to owner snapshot."
                },
                if self.data.snapshot.moderation.require_authorized_fetch {
                    "On"
                } else {
                    "Review"
                },
                if self.data.snapshot.moderation.require_authorized_fetch {
                    "ok"
                } else {
                    "warn"
                },
                "",
                "",
            ),
        ]
    }

    fn stats_rows(&self) -> Vec<UiRow> {
        let stats = &self.data.stats;
        vec![
            row(
                "stats:people",
                "People graph",
                "Owner-visible counts",
                &format!(
                    "{} followers, {} approved, {} pending, {} following",
                    stats.followers_total,
                    stats.followers_approved,
                    stats.followers_pending,
                    stats.following_total
                ),
                "Private",
                "ok",
                "",
                "",
            ),
            row(
                "stats:posts",
                "Posts",
                "Visibility mix",
                &format!(
                    "{} total, {} public, {} private, {} direct, {} encrypted",
                    stats.posts_total,
                    stats.public_posts,
                    stats.private_posts,
                    stats.direct_posts,
                    stats.encrypted_posts
                ),
                "Posting",
                "info",
                "",
                "",
            ),
            row(
                "stats:deliveries",
                "Deliveries",
                "Operational health",
                &format!(
                    "{} total, {} failed, {} queued, {} retrying",
                    stats.deliveries_total,
                    stats.deliveries_failed,
                    stats.deliveries_queued,
                    stats.deliveries_retry
                ),
                if stats.deliveries_failed > 0 {
                    "Attention"
                } else {
                    "OK"
                },
                if stats.deliveries_failed > 0 {
                    "warn"
                } else {
                    "ok"
                },
                "Inspect delivery",
                "",
            ),
        ]
    }

    fn inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let mut rows = Vec::new();
        if let Some(selected) = self.find_row(selected_row) {
            rows.push(row(
                &selected.id,
                &selected.title,
                &selected.subtitle,
                &selected.detail,
                &selected.chip,
                &selected.tone,
                &selected.primary,
                &selected.secondary,
            ));
        }
        rows.push(row(
            "inspector:privacy",
            "Visibility consequences",
            "Private by default",
            "Posts, follows, watches, and groups expose different information. Follow may notify a remote account; Watch does not.",
            "Safety",
            "ok",
            "",
            "",
        ));
        rows.push(row(
            "inspector:raw",
            "Advanced details",
            "Hidden by default",
            "Raw ActivityPub, AT Protocol, delivery IDs, and provider payloads belong in Diagnostics, not normal reading rows.",
            "Debug",
            "info",
            "Copy evidence",
            "",
        ));
        rows
    }

    fn find_row(&self, row_id: &str) -> Option<UiRow> {
        self.rows_for_active_screen()
            .into_iter()
            .find(|row| row.id.as_str() == row_id)
            .or_else(|| {
                self.home_today_rows()
                    .into_iter()
                    .find(|row| row.id.as_str() == row_id)
            })
            .or_else(|| {
                self.find_rows()
                    .into_iter()
                    .find(|row| row.id.as_str() == row_id)
            })
            .or_else(|| {
                self.delivery_rows()
                    .into_iter()
                    .find(|row| row.id.as_str() == row_id)
            })
    }

    fn first_row_id(&self) -> String {
        self.rows_for_active_screen()
            .first()
            .map(|row| row.id.to_string())
            .unwrap_or_default()
    }
}

trait IfEmpty {
    fn if_empty_else(self, fallback: impl FnOnce() -> String) -> String;
}

impl IfEmpty for String {
    fn if_empty_else(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

trait ModerationCounts {
    fn reply_queue_count(&self) -> usize;
}

impl ModerationCounts for ModerationState {
    fn reply_queue_count(&self) -> usize {
        self.reply_queue_count as usize
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let controller = Rc::new(RefCell::new(DeskController::load_default()?));
    let window = build_window(controller)?;
    window.run()?;
    Ok(())
}

pub fn build_window(
    controller: Rc<RefCell<DeskController>>,
) -> Result<MainWindow, slint::PlatformError> {
    let window = MainWindow::new()?;
    wire_callbacks(&window, controller.clone());
    apply_controller_projection(&window, &controller);
    Ok(window)
}

fn wire_callbacks(window: &MainWindow, controller: Rc<RefCell<DeskController>>) {
    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_select_mode(move |mode| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().select_mode(mode.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_select_screen(move |screen| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().select_screen(screen.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_select_row(move |row_id| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().select_row(row_id.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_row_action(move |row_id, action| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut()
                .row_action(row_id.as_str(), action.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_refresh(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().refresh();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_run_command(move |command| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().run_command(command.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_save_account(move |label, url, token| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut()
                .save_account(label.as_str(), url.as_str(), token.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_switch_account(move |account_id| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().switch_account(account_id.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_delete_account(move |account_id| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().delete_account(account_id.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_compose_set_visibility(move |value| {
        if let Some(window) = weak.upgrade() {
            {
                let mut controller = ctrl.borrow_mut();
                controller.update_compose_from_ui(
                    window.get_compose_text().as_str(),
                    window.get_compose_recipients().as_str(),
                    window.get_compose_media_description().as_str(),
                    window.get_compose_encrypt(),
                );
                controller.compose_set_visibility(value.as_str());
            }
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_compose_set_protocol(move |value| {
        if let Some(window) = weak.upgrade() {
            {
                let mut controller = ctrl.borrow_mut();
                controller.update_compose_from_ui(
                    window.get_compose_text().as_str(),
                    window.get_compose_recipients().as_str(),
                    window.get_compose_media_description().as_str(),
                    window.get_compose_encrypt(),
                );
                controller.compose_set_protocol(value.as_str());
            }
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller;
    window.on_compose_send(move || {
        if let Some(window) = weak.upgrade() {
            {
                let mut controller = ctrl.borrow_mut();
                controller.update_compose_from_ui(
                    window.get_compose_text().as_str(),
                    window.get_compose_recipients().as_str(),
                    window.get_compose_media_description().as_str(),
                    window.get_compose_encrypt(),
                );
                controller.compose_send();
            }
            apply_controller_projection(&window, &ctrl);
        }
    });
}

pub fn create_test_window() -> Result<MainWindow, slint::PlatformError> {
    let controller = Rc::new(RefCell::new(DeskController::fixture_for_tests()));
    let window = MainWindow::new()?;
    wire_callbacks(&window, controller.clone());
    apply_controller_projection(&window, &controller);
    Ok(window)
}

fn apply_controller_projection(window: &MainWindow, controller: &Rc<RefCell<DeskController>>) {
    let projection = controller.borrow().projection();
    apply_projection_data(window, projection);
}

fn apply_projection_data(window: &MainWindow, projection: UiProjection) {
    window.set_mode_nav(model(projection.mode_nav));
    window.set_screen_nav(model(projection.screen_nav));
    window.set_rows(model(projection.rows));
    window.set_inspector_rows(model(projection.inspector_rows));
    window.set_accounts(model(projection.accounts));
    window.set_active_mode(s(&projection.active_mode));
    window.set_active_screen(s(&projection.active_screen));
    window.set_selected_row(s(&projection.selected_row));
    window.set_window_title(s(&projection.window_title));
    window.set_window_subtitle(s(&projection.window_subtitle));
    window.set_attention_summary(s(&projection.attention_summary));
    window.set_privacy_status(s(&projection.privacy_status));
    window.set_status_message(s(&projection.status_message));
    window.set_command_text(s(&projection.command_text));
    window.set_compose_text(s(&projection.compose_text));
    window.set_compose_recipients(s(&projection.compose_recipients));
    window.set_compose_media_description(s(&projection.compose_media_description));
    window.set_compose_encrypt(projection.compose_encrypt);
    window.set_compose_visibility(s(&projection.compose_visibility));
    window.set_compose_protocol(s(&projection.compose_protocol));
    window.set_compose_warning(s(&projection.compose_warning));
    window.set_compose_can_send(projection.compose_can_send);
    window.set_account_label(s(&projection.account_label));
    window.set_account_url(s(&projection.account_url));
    window.set_account_token(s(&projection.account_token));
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

fn s(value: &str) -> SharedString {
    SharedString::from(value)
}

fn nav(id: &str, title: &str, count: usize, active: bool) -> NavItem {
    NavItem {
        id: s(id),
        title: s(title),
        count: if count == 0 {
            s("")
        } else {
            s(&count.to_string())
        },
        active,
    }
}

fn row(
    id: &str,
    title: &str,
    subtitle: &str,
    detail: &str,
    chip: &str,
    tone: &str,
    primary: &str,
    secondary: &str,
) -> UiRow {
    UiRow {
        id: s(id),
        title: s(&clean_text(title)),
        subtitle: s(&clean_text(subtitle)),
        detail: s(&clean_text(detail)),
        chip: s(chip),
        tone: s(tone),
        primary: s(primary),
        secondary: s(secondary),
    }
}

fn account_row(account: OwnerAccountSummary) -> AccountRow {
    AccountRow {
        id: s(&account.id),
        title: s(&account.label),
        subtitle: s(&account.instance_url),
        active: account.active,
        token: account.owner_token_present,
    }
}

fn timeline_row(post: &OwnerTimelinePost) -> UiRow {
    let author = post
        .actor_display_name
        .as_deref()
        .or(post.actor_username.as_deref())
        .unwrap_or(&post.actor_id);
    row(
        &format!("timeline:{}", post.object_id),
        author,
        post.actor_username.as_deref().unwrap_or(&post.actor_id),
        post.content_html.as_deref().unwrap_or(&post.content),
        visibility_string_label(&post.visibility),
        visibility_tone(&post.visibility),
        "Reply",
        "Favorite",
    )
}

fn post_row(post: &OwnerPost) -> UiRow {
    let title = post.title.as_deref().unwrap_or("My post");
    row(
        &format!("post:{}", post.id),
        title,
        &format!(
            "{} via {}",
            visibility_label(&post.visibility),
            protocol_label(&post.protocol)
        ),
        &post.content,
        visibility_label(&post.visibility),
        visibility_tone_enum(&post.visibility),
        "Reply",
        if matches!(post.visibility, Visibility::Public) {
            "Delete"
        } else {
            "Favorite"
        },
    )
}

fn notification_row(notice: &OwnerNotification) -> UiRow {
    let actor = notice
        .actor_display_name
        .as_deref()
        .or(notice.actor_username.as_deref())
        .unwrap_or(&notice.actor_id);
    let title = match notice.kind.as_str() {
        "mention" => format!("Mention from {actor}"),
        "reply" => format!("Reply from {actor}"),
        "favourite" | "favorite" | "like" => format!("Like from {actor}"),
        "repost" | "boost" => format!("Boost from {actor}"),
        "follow" => format!("Follow request from {actor}"),
        kind => format!("{kind} from {actor}"),
    };
    let context = notice
        .context_post_content_html
        .as_deref()
        .or(notice.context_post_content.as_deref())
        .or(notice.content.as_deref())
        .unwrap_or("Open context to inspect the related post.");
    row(
        &format!("notification:{}", notice.id),
        &title,
        notice.created_at.as_deref().unwrap_or("Notification"),
        context,
        if json_truthy(&notice.read) {
            "Read"
        } else {
            "Unread"
        },
        if json_truthy(&notice.read) {
            "info"
        } else {
            "warn"
        },
        if json_truthy(&notice.read) {
            "Open context"
        } else {
            "Mark read"
        },
        "Reply",
    )
}

fn dm_row(dm: &OwnerDirectMessage) -> UiRow {
    row(
        &format!("dm:{}", dm.id),
        &format!("Direct message from {}", dm.sender_id),
        &dm.published_at,
        &dm.content,
        "Direct",
        "ok",
        "Reply",
        "Open context",
    )
}

fn follower_row(follower: &OwnerFollower) -> UiRow {
    let (primary, secondary, tone) = match follower.status.as_str() {
        "pending" => ("Approve", "Reject", "warn"),
        "approved" | "accepted" => ("", "Remove", "ok"),
        "rejected" => ("Approve", "", "danger"),
        _ => ("Approve", "Reject", "info"),
    };
    row(
        &format!("follower:{}", follower.follower_actor_id),
        &follower.follower_actor_id,
        "Can read private posts only if approved",
        &format!(
            "Status: {}. Inbox details are hidden unless diagnostics are opened.",
            follower.status
        ),
        &follower.status,
        tone,
        primary,
        secondary,
    )
}

fn friend_row(friend: &OwnerFriend) -> UiRow {
    row(
        &format!("actor:{}", friend.friend_actor_id),
        &friend.friend_actor_id,
        "Mutual private sharing",
        "Friend means both sides can participate in the private social graph. Manage group membership from Audience Groups.",
        "Friend",
        "ok",
        "Open context",
        "Block",
    )
}

fn following_row(following: &OwnerFollowing) -> UiRow {
    let (primary, secondary, tone) = match following.status.as_str() {
        "accepted" | "approved" => ("Unfollow", "", "ok"),
        "pending" => ("Cancel", "", "warn"),
        "failed" => ("Follow", "", "danger"),
        _ => ("Unfollow", "", "info"),
    };
    row(
        &format!("following:{}", following.target_actor_id),
        &following.target_actor_id,
        "Remote relationship may be visible to that server",
        &format!("Follow status: {}", following.status),
        &following.status,
        tone,
        primary,
        secondary,
    )
}

fn discovered_actor_row(actor: &OwnerDiscoveredActor) -> UiRow {
    let title = actor
        .name
        .as_deref()
        .or(actor.preferred_username.as_deref())
        .or(actor.handle.as_deref())
        .unwrap_or(&actor.id);
    row(
        &format!("actor:{}", actor.id),
        title,
        actor.handle.as_deref().unwrap_or(&actor.id),
        actor.summary.as_deref().unwrap_or(
            "Discovered account. Follow may notify; Watch reads public posts privately.",
        ),
        actor.following_status.as_deref().unwrap_or("Unknown"),
        "info",
        "Follow",
        "Watch",
    )
}

fn public_actor_row(actor: &OwnerPublicSearchActor) -> UiRow {
    row(
        &format!(
            "actor:{}",
            actor.follow_target.as_deref().unwrap_or(&actor.id)
        ),
        actor
            .display_name
            .as_deref()
            .or(actor.handle.as_deref())
            .unwrap_or(&actor.id),
        &format!("{} via {}", actor.network, actor.provider),
        actor
            .summary
            .as_deref()
            .unwrap_or("Public search actor result. Choose Follow or Watch deliberately."),
        "Public result",
        "info",
        if actor.actions.iter().any(|a| a == "follow") {
            "Follow"
        } else {
            ""
        },
        if actor.actions.iter().any(|a| a == "watch") {
            "Watch"
        } else {
            ""
        },
    )
}

fn public_post_row(post: &OwnerPublicSearchPost) -> UiRow {
    row(
        &format!("url:{}", post.url),
        post.actor_display_name
            .as_deref()
            .or(post.actor_handle.as_deref())
            .unwrap_or("Public post"),
        &format!("{} via {}", post.network, post.provider),
        post.content_html.as_deref().unwrap_or(&post.content),
        "Public",
        "warn",
        "Open original",
        if post.watch_target.is_some() {
            "Watch"
        } else {
            ""
        },
    )
}

fn source_subscription_row(source: &SourceSubscription) -> UiRow {
    row(
        &format!("source:{}", source.id),
        source.title.as_deref().unwrap_or(&source.url),
        &format!("{} source", source.source_type),
        &format!(
            "{}. Last fetched: {}. Errors: {}.",
            source.status,
            source.last_fetched_at.as_deref().unwrap_or("never"),
            source.error_count
        ),
        &source.status,
        if source.status == "active" {
            "ok"
        } else {
            "warn"
        },
        "Refresh",
        "Stop watching",
    )
}

fn watch_subscription_row(source: &SourceSubscription) -> UiRow {
    let mut row = source_subscription_row(source);
    row.id = s(&format!("watch:{}", source.id));
    row.subtitle = s("Private public-post watch. Remote account is not notified.");
    row.chip = s("Watch");
    row
}

fn source_item_row(item: &SourceItem) -> UiRow {
    row(
        &format!("source-item:{}", item.id),
        &item.title,
        &item.source_type,
        item.excerpt
            .as_deref()
            .or(item.canonical_url.as_deref())
            .unwrap_or("Source item"),
        "Source item",
        "info",
        "Open link",
        "",
    )
}

fn search_source_item_row(item: &dais_client_core::OwnerSearchSourceItem) -> UiRow {
    row(
        &format!("source-item:{}", item.id),
        &item.title,
        &item.source_type,
        item.excerpt
            .as_deref()
            .or(item.canonical_url.as_deref())
            .unwrap_or("Search source item"),
        "Source item",
        "info",
        "Open link",
        "",
    )
}

fn audience_row(list: &OwnerAudienceList) -> UiRow {
    row(
        &format!("audience:{}", list.id),
        &list.name,
        &format!("{} members", list.member_count),
        list.description
            .as_deref()
            .unwrap_or("Audience groups are owner-controlled sharing sets."),
        "Audience",
        "ok",
        "Open context",
        "Remove",
    )
}

fn diagnostic_row(diagnostic: &DiagnosticStatus) -> UiRow {
    row(
        &format!("diagnostic:{}", diagnostic.key),
        &diagnostic.key,
        if diagnostic.ok {
            "OK"
        } else {
            "Needs attention"
        },
        &diagnostic.detail,
        if diagnostic.ok { "OK" } else { "Issue" },
        if diagnostic.ok { "ok" } else { "warn" },
        if diagnostic.ok { "" } else { "Copy evidence" },
        "",
    )
}

fn delivery_attention_row(delivery: &OwnerDelivery) -> UiRow {
    let mut row = delivery_row(delivery);
    row.primary = s("Inspect delivery");
    row
}

fn delivery_row(delivery: &OwnerDelivery) -> UiRow {
    let tone = match delivery.status.as_str() {
        "failed" => "danger",
        "delivered" => "ok",
        "retry" | "queued" => "warn",
        _ => "info",
    };
    row(
        &format!("delivery:{}", delivery.id),
        &format!("{} delivery", delivery.protocol),
        &delivery
            .target_type
            .clone()
            .unwrap_or_else(|| "recipient".into()),
        &format!(
            "{} to {}. {}",
            delivery.status,
            compact_url(&delivery.target_url),
            delivery.error_message.as_deref().unwrap_or("")
        ),
        &delivery.status,
        tone,
        if delivery.status == "failed" {
            "Retry"
        } else {
            ""
        },
        "Open context",
    )
}

fn moderation_reply_row(reply: &ModerationReplyRow) -> UiRow {
    let status = reply.moderation_status.as_deref().unwrap_or("needs review");
    row(
        &format!("moderation-reply:{}", reply.id),
        reply
            .actor_display_name
            .as_deref()
            .or(reply.actor_username.as_deref())
            .unwrap_or(&reply.actor_id),
        status,
        &reply.content,
        if reply.moderation_flags.is_empty() {
            "Review"
        } else {
            "Flagged"
        },
        if reply.moderation_flags.is_empty() {
            "warn"
        } else {
            "danger"
        },
        "Approve reply",
        "Hide reply",
    )
}

fn clean_text(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skipping = false;
    let mut skip_until = String::new();

    for ch in decode_html_entities(value).chars() {
        if skipping {
            tag.push(ch.to_ascii_lowercase());
            if tag.ends_with(&skip_until) {
                skipping = false;
                tag.clear();
            }
            continue;
        }
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                let lower = tag.to_ascii_lowercase();
                if lower.starts_with("script") {
                    skipping = true;
                    skip_until = "</script>".to_string();
                } else if lower.starts_with("style") {
                    skipping = true;
                    skip_until = "</style>".to_string();
                } else if lower.starts_with("br")
                    || lower.starts_with("/p")
                    || lower.starts_with("p")
                {
                    output.push(' ');
                }
                in_tag = false;
                tag.clear();
            }
            _ if in_tag => tag.push(ch),
            _ => output.push(ch),
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn extract_first_url(value: &str) -> Option<String> {
    value
        .split_whitespace()
        .find(|part| part.starts_with("https://") || part.starts_with("http://"))
        .map(|part| part.trim_end_matches(&[',', '.', ')', ']'][..]).to_string())
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg("start");
        cmd
    };
    command.arg(url);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn related_context(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("notification:")
        .or_else(|| row_id.strip_prefix("delivery:"))
        .or_else(|| row_id.strip_prefix("dm:"))
}

fn object_id_from_row(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("post:")
        .or_else(|| row_id.strip_prefix("timeline:"))
        .or_else(|| row_id.strip_prefix("url:"))
}

fn target_from_row(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("actor:")
        .or_else(|| row_id.strip_prefix("following:"))
        .or_else(|| row_id.strip_prefix("follower:"))
        .or_else(|| row_id.strip_prefix("url:"))
}

fn mode_for_screen(screen: &str) -> &str {
    match screen {
        "find" | "relationship" | "friends" | "followers" | "following" | "watches"
        | "audience" | "blocks" => "people",
        "health" | "deliveries" | "moderation" | "identity" | "accounts" | "settings" | "stats" => {
            "server"
        }
        _ => "home",
    }
}

fn visibility_label(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "Public",
        Visibility::Unlisted => "Unlisted",
        Visibility::Followers => "Followers",
        Visibility::Direct => "Direct",
    }
}

fn visibility_string_label(visibility: &str) -> &str {
    match visibility {
        "public" => "Public",
        "unlisted" => "Unlisted",
        "direct" => "Direct",
        "followers" | "private" => "Followers",
        value => value,
    }
}

fn visibility_tone(visibility: &str) -> &'static str {
    match visibility {
        "public" => "warn",
        "direct" | "followers" | "private" => "ok",
        _ => "info",
    }
}

fn visibility_tone_enum(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "warn",
        Visibility::Direct | Visibility::Followers => "ok",
        Visibility::Unlisted => "info",
    }
}

fn protocol_label(protocol: &ProtocolRoute) -> &'static str {
    match protocol {
        ProtocolRoute::ActivityPub => "ActivityPub",
        ProtocolRoute::AtProto => "Bluesky",
        ProtocolRoute::Both => "Both",
    }
}

fn compose_warning(compose: &ComposeState) -> String {
    if compose.text.trim().is_empty() {
        return "Write a post before sending.".into();
    }
    if matches!(compose.visibility, Visibility::Direct)
        && split_list(&compose.recipients).is_empty()
    {
        return "Direct posts require named recipients.".into();
    }
    if matches!(compose.visibility, Visibility::Public) {
        return "This will be public. Use Post Publicly only when that is intentional.".into();
    }
    if matches!(
        compose.protocol,
        ProtocolRoute::AtProto | ProtocolRoute::Both
    ) && !matches!(compose.visibility, Visibility::Public)
    {
        return "Private ActivityPub visibility is not representable on Bluesky.".into();
    }
    "Ready to send privately.".into()
}

fn compose_can_send(compose: &ComposeState) -> bool {
    !compose.text.trim().is_empty()
        && (!matches!(compose.visibility, Visibility::Direct)
            || !split_list(&compose.recipients).is_empty())
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(&[',', '\n'][..])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn looks_like_handle_or_url(value: &str) -> bool {
    value.starts_with('@')
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.contains('@')
}

fn json_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_u64().unwrap_or_default() != 0,
        serde_json::Value::String(value) => matches!(value.as_str(), "true" | "1"),
        _ => false,
    }
}

fn compact_url(url: &str) -> String {
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    if trimmed.len() > 64 {
        format!("{}...", &trimmed[..61])
    } else {
        trimmed.to_string()
    }
}

fn fixture_data(api_error: Option<String>) -> DeskData {
    let settings = StoredOwnerSettings::default();
    let snapshot = local_snapshot(settings, api_error.clone()).into();
    DeskData {
        snapshot,
        notifications: vec![
            OwnerNotification {
                id: "notice-like-context".into(),
                kind: "like".into(),
                actor_id: "https://science.example/users/research".into(),
                actor_username: Some("research".into()),
                actor_display_name: Some("Research Desk".into()),
                actor_avatar_url: None,
                post_id: Some("fixture-private-post".into()),
                activity_id: Some("activity-hidden-in-ui".into()),
                content: Some("<p>liked your note</p>".into()),
                read: serde_json::Value::Bool(false),
                created_at: Some("now".into()),
                context_post_id: Some("fixture-private-post".into()),
                context_post_content: Some("This is a private post with context.".into()),
                context_post_content_html: Some("<p>This is a <b>private</b> post with <a href=\"https://dais.social\">context</a>.</p>".into()),
                context_post_visibility: Some("followers".into()),
                context_post_protocol: Some("activitypub".into()),
                context_post_published_at: Some("today".into()),
            },
            OwnerNotification {
                id: "notice-reply".into(),
                kind: "reply".into(),
                actor_id: "https://friend.example/users/ada".into(),
                actor_username: Some("ada".into()),
                actor_display_name: Some("Ada Friend".into()),
                actor_avatar_url: None,
                post_id: Some("fixture-private-post".into()),
                activity_id: None,
                content: Some("Can we keep this to close friends?".into()),
                read: serde_json::Value::Bool(false),
                created_at: Some("5m".into()),
                context_post_id: Some("fixture-private-post".into()),
                context_post_content: Some("Original close-friends post".into()),
                context_post_content_html: None,
                context_post_visibility: Some("followers".into()),
                context_post_protocol: Some("activitypub".into()),
                context_post_published_at: Some("today".into()),
            },
        ],
        deliveries: vec![
            OwnerDelivery {
                id: "delivery-failed".into(),
                post_id: "fixture-private-post".into(),
                target_type: Some("shared inbox".into()),
                target_url: "https://remote.example/inbox".into(),
                protocol: "ActivityPub".into(),
                status: "failed".into(),
                retry_count: Some(2),
                last_attempt_at: Some("now".into()),
                error_message: Some("Remote host returned 502".into()),
                activity_type: Some("Create".into()),
                created_at: Some("today".into()),
                delivered_at: None,
            },
            OwnerDelivery {
                id: "delivery-ok".into(),
                post_id: "fixture-public-photo".into(),
                target_type: Some("Bluesky repo".into()),
                target_url: "at://did:example/app.bsky.feed.post/123".into(),
                protocol: "Bluesky".into(),
                status: "delivered".into(),
                retry_count: Some(0),
                last_attempt_at: Some("today".into()),
                error_message: None,
                activity_type: Some("Create".into()),
                created_at: Some("today".into()),
                delivered_at: Some("today".into()),
            },
        ],
        direct_messages: vec![OwnerDirectMessage {
            id: "dm-fixture".into(),
            conversation_id: "conversation-ada".into(),
            sender_id: "https://friend.example/users/ada".into(),
            content: "This should stay direct.".into(),
            published_at: "today".into(),
            created_at: Some("today".into()),
        }],
        sources: OwnerSources {
            subscriptions: vec![SourceSubscription {
                id: "source-npr".into(),
                source_type: "rss".into(),
                url: "https://www.npr.org/rss/rss.php?id=1001".into(),
                title: Some("NPR News".into()),
                homepage_url: Some("https://www.npr.org".into()),
                status: "active".into(),
                refresh_cadence_minutes: 60,
                last_fetched_at: Some("today".into()),
                next_fetch_at: Some("soon".into()),
                last_error: None,
                error_count: 0,
                policy_json: "{\"private_reader_only\":true}".into(),
                created_at: Some("today".into()),
                updated_at: Some("today".into()),
            }],
            items: vec![SourceItem {
                id: "source-item-science".into(),
                title: "Science source item".into(),
                source_type: "rss".into(),
                canonical_url: Some("https://example.org/science".into()),
                excerpt: Some("A public science update saved for private reading.".into()),
                rights_policy_json: "{\"excerpt_only\":true}".into(),
                read: false,
            }],
        },
        watches: OwnerSources {
            subscriptions: vec![SourceSubscription {
                id: "watch-nobel".into(),
                source_type: "activitypub".into(),
                url: "https://social.example/users/nobel".into(),
                title: Some("Nobel Prize public posts".into()),
                homepage_url: Some("https://www.nobelprize.org".into()),
                status: "active".into(),
                refresh_cadence_minutes: 120,
                last_fetched_at: Some("today".into()),
                next_fetch_at: Some("later".into()),
                last_error: None,
                error_count: 0,
                policy_json: "{\"private_reader_only\":true}".into(),
                created_at: Some("today".into()),
                updated_at: Some("today".into()),
            }],
            items: Vec::new(),
        },
        moderation_replies: vec![ModerationReplyRow {
            id: "mod-reply-sensitive".into(),
            post_id: "fixture-private-post".into(),
            actor_id: "https://unknown.example/users/hot-take".into(),
            actor_username: Some("hot-take".into()),
            actor_display_name: Some("Hot Take".into()),
            actor_avatar_url: None,
            content: "This reply may be too sensitive for the public thread.".into(),
            published_at: Some("today".into()),
            created_at: Some("today".into()),
            moderation_status: Some("needs_review".into()),
            moderation_score: Some(0.73),
            moderation_flags: vec!["politics".into()],
            moderation_checked_at: Some("today".into()),
            hidden: serde_json::Value::Bool(false),
        }],
        stats: OwnerStats {
            followers_total: 4,
            followers_approved: 3,
            followers_pending: 1,
            followers_rejected: 0,
            following_total: 5,
            posts_total: 8,
            activities_total: 20,
            deliveries_total: 12,
            deliveries_failed: 1,
            deliveries_queued: 0,
            deliveries_retry: 1,
            deliveries_delivered: 10,
            dual_protocol_posts: 2,
            public_posts: 2,
            private_posts: 5,
            direct_posts: 1,
            encrypted_posts: 1,
            media_posts: 2,
            notifications_unread: 2,
            blocks_total: 1,
            allowlist_hosts: 1,
            closed_network: false,
        },
        search: OwnerSearchResult::default(),
        discovered_actor: None,
        api_error,
    }
}

fn fixture_search(query: &str) -> OwnerSearchResult {
    OwnerSearchResult {
        public_posts: vec![OwnerPublicSearchPost {
            provider: "tootfinder".into(),
            network: "ActivityPub".into(),
            id: "public-result-1".into(),
            url: "https://mastodon.example/@science/123".into(),
            content: format!("Public result for {query}. Links stay clickable via Open original."),
            canonical_url: Some("https://mastodon.example/@science/123".into()),
            actor_id: Some("https://mastodon.example/users/science".into()),
            actor_handle: Some("@science@mastodon.example".into()),
            actor_display_name: Some("Science Example".into()),
            content_html: Some(
                "<p>Public result with <a href=\"https://example.org\">link</a></p>".into(),
            ),
            summary: None,
            object_type: Some("Note".into()),
            published_at: Some("today".into()),
            watch_type: Some("activitypub".into()),
            watch_target: Some("https://mastodon.example/users/science".into()),
            reply_target: Some("https://mastodon.example/@science/123".into()),
            actions: vec!["watch".into()],
            cid: None,
            reply_count: Some(1),
            repost_count: Some(2),
            like_count: Some(3),
        }],
        public_actors: vec![OwnerPublicSearchActor {
            provider: "public-index".into(),
            network: "ActivityPub".into(),
            id: "https://mastodon.example/users/science".into(),
            handle: Some("@science@mastodon.example".into()),
            display_name: Some("Science Example".into()),
            summary: Some("Public science account.".into()),
            url: Some("https://mastodon.example/@science".into()),
            avatar_url: None,
            watch_type: Some("activitypub".into()),
            watch_target: Some("https://mastodon.example/users/science".into()),
            follow_target: Some("https://mastodon.example/users/science".into()),
            actions: vec!["follow".into(), "watch".into()],
        }],
        ..OwnerSearchResult::default()
    }
}

fn local_snapshot(
    stored: StoredOwnerSettings,
    api_error: Option<String>,
) -> dais_client_core::OwnerSnapshot {
    let owner_token_present = stored
        .owner_token
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let owner_api_ok = api_error.is_none() && owner_token_present;
    dais_client_core::OwnerSnapshot {
        settings: OwnerSettings {
            instance_url: stored.instance_url,
            owner_token_present,
            default_visibility: Visibility::Followers,
            default_protocol: ProtocolRoute::ActivityPub,
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
        home_timeline: vec![OwnerTimelinePost {
            id: "timeline-fixture".into(),
            object_id: "fixture-private-post".into(),
            actor_id: "https://friend.example/users/ada".into(),
            actor_username: Some("ada".into()),
            actor_display_name: Some("Ada Friend".into()),
            actor_avatar_url: None,
            content: "A private friend post that can be replied to without changing audience.".into(),
            content_html: Some("<p>A <b>private</b> friend post that can be replied to safely.</p>".into()),
            visibility: "followers".into(),
            in_reply_to: None,
            published_at: Some("today".into()),
            protocol: Some("activitypub".into()),
            reply_count: 2,
            like_count: 1,
            boost_count: 0,
        }],
        posts: vec![
            OwnerPost {
                id: "fixture-private-post".to_string(),
                title: Some("Private launch note".to_string()),
                content: "Private-by-default compose, replies, follows, watches, moderation, delivery, diagnostics, and profile screens are available from the native client.".to_string(),
                visibility: Visibility::Followers,
                protocol: ProtocolRoute::ActivityPub,
                encrypted: false,
                attachments: Vec::new(),
                reply_count: 1,
                like_count: 1,
                boost_count: 0,
                published_at: Some("today".into()),
            },
            OwnerPost {
                id: "fixture-public-photo".to_string(),
                title: Some("Public media demo".to_string()),
                content: "A public post is intentionally marked public before sending.".to_string(),
                visibility: Visibility::Public,
                protocol: ProtocolRoute::Both,
                encrypted: false,
                attachments: Vec::new(),
                reply_count: 0,
                like_count: 3,
                boost_count: 1,
                published_at: Some("today".into()),
            },
        ],
        followers: vec![
            OwnerFollower {
                id: "follower-pending".into(),
                actor_id: "https://social.dais.social/users/social".into(),
                follower_actor_id: "https://new.example/users/follower".into(),
                follower_inbox: "https://new.example/inbox".into(),
                follower_shared_inbox: None,
                status: "pending".into(),
                created_at: Some("today".into()),
                updated_at: Some("today".into()),
            },
            OwnerFollower {
                id: "follower-approved".into(),
                actor_id: "https://social.dais.social/users/social".into(),
                follower_actor_id: "https://friend.example/users/ada".into(),
                follower_inbox: "https://friend.example/inbox".into(),
                follower_shared_inbox: None,
                status: "approved".into(),
                created_at: Some("today".into()),
                updated_at: Some("today".into()),
            },
        ],
        friends: vec![OwnerFriend {
            friend_actor_id: "https://friend.example/users/ada".into(),
            friend_inbox: Some("https://friend.example/inbox".into()),
            friend_shared_inbox: None,
            follower_since: Some("today".into()),
            following_since: Some("today".into()),
            accepted_at: Some("today".into()),
        }],
        following: vec![OwnerFollowing {
            id: "following-science".into(),
            actor_id: "https://social.dais.social/users/social".into(),
            target_actor_id: "https://science.example/users/news".into(),
            target_inbox: "https://science.example/inbox".into(),
            status: "accepted".into(),
            created_at: Some("today".into()),
            accepted_at: Some("today".into()),
        }],
        audience_lists: vec![OwnerAudienceList {
            id: "close-friends".into(),
            name: "Close Friends".into(),
            description: Some("Small group for sensitive personal posts.".into()),
            allowed_categories: vec!["personal".into(), "medical".into()],
            member_actor_ids: vec!["https://friend.example/users/ada".into()],
            member_count: 1,
            created_at: Some("today".into()),
            updated_at: Some("today".into()),
        }],
        sources: vec![SourceItem {
            id: "sources-ready".to_string(),
            title: "Public source reader".to_string(),
            source_type: "rss/atom/api".to_string(),
            canonical_url: Some("https://dais.social".to_string()),
            excerpt: Some("Reads normalized private source items once the owner API is wired.".to_string()),
            rights_policy_json: "{\"private_reader_only\":true,\"excerpt_only\":true}".to_string(),
            read: false,
        }],
        moderation: ModerationState {
            closed_network: false,
            block_count: 1,
            allowlist_count: 1,
            require_authorized_fetch: true,
            manually_approves_followers: true,
            reply_policy: "warn".to_string(),
            ai_enabled: false,
            ai_model: Some("@cf/meta/llama-guard-3-8b".to_string()),
            ai_daily_budget: 0,
            reply_queue_count: 1,
            flagged_reply_count: 1,
            hidden_reply_count: 0,
            rejected_reply_count: 0,
            blocks: vec![dais_client_core::ModerationBlockRow {
                id: "block-spam".into(),
                actor_id: "https://spam.example/users/bad".into(),
                blocked_domain: Some("spam.example".into()),
                reason: Some("Spam replies".into()),
                created_at: Some("today".into()),
            }],
            allowlist: vec![dais_client_core::ModerationAllowlistHost {
                host: "friend.example".into(),
                note: Some("Trusted friend server".into()),
                enabled: serde_json::Value::Bool(true),
                created_at: Some("today".into()),
                updated_at: Some("today".into()),
            }],
        },
        diagnostics: vec![
            DiagnosticStatus {
                key: "owner-api".to_string(),
                ok: owner_api_ok,
                detail: api_error.unwrap_or_else(|| {
                    "No owner API token stored; showing native Slint preview data.".to_string()
                }),
            },
            DiagnosticStatus {
                key: "slint-native-ui".to_string(),
                ok: true,
                detail: "Rust-native Slint UI is active; legacy WebView owner app code has been removed."
                    .to_string(),
            },
        ],
    }
}

fn default_instance_url() -> String {
    DEFAULT_INSTANCE_URL.to_string()
}

fn load_settings_from(path: &PathBuf) -> Result<StoredOwnerSettings, String> {
    if !path.exists() {
        if let Some(settings) = load_legacy_settings_for(path)? {
            persist_settings_to(path, settings.clone())?;
            return Ok(settings);
        }
        return Ok(StoredOwnerSettings::default());
    }
    read_settings_file(path)
}

fn read_settings_file(path: &PathBuf) -> Result<StoredOwnerSettings, String> {
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let settings: StoredOwnerSettings =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    Ok(normalize_settings(settings))
}

fn load_legacy_settings_for(path: &PathBuf) -> Result<Option<StoredOwnerSettings>, String> {
    if std::env::var_os("DAIS_DESK_SETTINGS").is_some() {
        return Ok(None);
    }
    let Some(default_path) = platform_default_settings_path() else {
        return Ok(None);
    };
    if path != &default_path {
        return Ok(None);
    }
    let Some(legacy_path) = legacy_settings_path() else {
        return Ok(None);
    };
    if !legacy_path.exists() {
        return Ok(None);
    }
    read_settings_file(&legacy_path).map(Some)
}

fn persist_settings_to(path: &PathBuf, settings: StoredOwnerSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

fn normalize_settings(mut settings: StoredOwnerSettings) -> StoredOwnerSettings {
    settings.instance_url =
        normalize_instance_url(&settings.instance_url).unwrap_or_else(default_instance_url);
    if settings.accounts.is_empty() {
        settings.accounts.push(StoredOwnerAccount {
            id: account_id_for(&settings.instance_url, &[]),
            label: account_label(&settings.instance_url),
            instance_url: settings.instance_url.clone(),
            owner_token: settings.owner_token.clone(),
        });
    }

    let mut existing_ids: Vec<String> = Vec::new();
    for account in &mut settings.accounts {
        account.instance_url =
            normalize_instance_url(&account.instance_url).unwrap_or_else(default_instance_url);
        account.label = optional_trimmed(&account.label)
            .unwrap_or_else(|| account_label(&account.instance_url));
        if account.id.trim().is_empty() || existing_ids.iter().any(|id| id == &account.id) {
            account.id = account_id_for(&account.instance_url, &existing_ids);
        }
        existing_ids.push(account.id.clone());
    }

    let active_id = settings
        .active_account_id
        .as_deref()
        .and_then(|id| settings.accounts.iter().find(|account| account.id == id))
        .map(|account| account.id.clone())
        .unwrap_or_else(|| settings.accounts[0].id.clone());
    settings.active_account_id = Some(active_id.clone());
    if let Some(account) = settings
        .accounts
        .iter()
        .find(|account| account.id == active_id)
    {
        settings.instance_url = account.instance_url.clone();
        settings.owner_token = account.owner_token.clone();
    }
    settings
}

fn account_summaries(settings: &StoredOwnerSettings) -> Vec<OwnerAccountSummary> {
    settings
        .accounts
        .iter()
        .map(|account| OwnerAccountSummary {
            id: account.id.clone(),
            label: account.label.clone(),
            instance_url: account.instance_url.clone(),
            active: settings.active_account_id.as_deref() == Some(account.id.as_str()),
            owner_token_present: account
                .owner_token
                .as_deref()
                .is_some_and(|value| !value.is_empty()),
        })
        .collect()
}

fn active_account(settings: &StoredOwnerSettings) -> Option<&StoredOwnerAccount> {
    settings
        .active_account_id
        .as_deref()
        .and_then(|id| settings.accounts.iter().find(|account| account.id == id))
        .or_else(|| settings.accounts.first())
}

fn normalize_instance_url(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("https://{trimmed}"))
    }
}

fn optional_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn account_label(instance_url: &str) -> String {
    let host = instance_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(instance_url);
    if host == "social.dais.social" {
        "Dais Social".to_string()
    } else {
        host.to_string()
    }
}

fn account_id_for(instance_url: &str, existing_ids: &[String]) -> String {
    let host = account_label(instance_url).to_lowercase();
    let mut slug = String::new();
    for character in host.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("dais");
    }
    let base = format!("account-{slug}");
    let mut candidate = base.clone();
    let mut suffix = 2;
    while existing_ids.iter().any(|id| id == &candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn default_settings_path() -> PathBuf {
    if let Ok(path) = std::env::var("DAIS_DESK_SETTINGS") {
        return PathBuf::from(path);
    }
    platform_default_settings_path().unwrap_or_else(|| PathBuf::from("owner-settings.json"))
}

fn platform_default_settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        Some(
            home.join("Library")
                .join("Application Support")
                .join("social.dais.desk")
                .join("owner-settings.json"),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(
            home.join(".config")
                .join("dais-desk")
                .join("owner-settings.json"),
        )
    }
}

fn legacy_settings_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DAIS_DESK_LEGACY_SETTINGS") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        Some(
            home.join("Library")
                .join("Application Support")
                .join("social.dais.owner")
                .join("owner-settings.json"),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(
            home.join(".config")
                .join("dais-owner")
                .join("owner-settings.json"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markup_and_script_content() {
        let cleaned = clean_text("<p>Hello <b>friend</b><script>alert(1)</script></p>");
        assert_eq!(cleaned, "Hello friend");
    }

    #[test]
    fn hides_redundant_follower_actions() {
        let row = follower_row(&OwnerFollower {
            id: "1".into(),
            actor_id: "me".into(),
            follower_actor_id: "them".into(),
            follower_inbox: "https://example.test/inbox".into(),
            follower_shared_inbox: None,
            status: "approved".into(),
            created_at: None,
            updated_at: None,
        });
        assert_eq!(row.primary.as_str(), "");
        assert_eq!(row.secondary.as_str(), "Remove");
    }

    #[test]
    fn compose_requires_direct_recipients() {
        let compose = ComposeState {
            text: "secret".into(),
            visibility: Visibility::Direct,
            ..ComposeState::default()
        };
        assert!(!compose_can_send(&compose));
        assert_eq!(
            compose_warning(&compose),
            "Direct posts require named recipients."
        );
    }

    #[test]
    fn normalizes_multi_account_settings() {
        let settings = normalize_settings(StoredOwnerSettings {
            instance_url: "joneslaw.io/".into(),
            owner_token: Some("token".into()),
            active_account_id: None,
            accounts: Vec::new(),
        });
        assert_eq!(settings.instance_url, "https://joneslaw.io");
        assert_eq!(
            settings.active_account_id.as_deref(),
            Some("account-joneslaw-io")
        );
    }

    #[test]
    fn migrates_legacy_owner_settings_to_desk_path() {
        let temp = tempfile::tempdir().expect("temp dir");
        let previous_home = std::env::var_os("HOME");
        let previous_settings = std::env::var_os("DAIS_DESK_SETTINGS");
        let previous_legacy = std::env::var_os("DAIS_DESK_LEGACY_SETTINGS");

        std::env::set_var("HOME", temp.path());
        std::env::remove_var("DAIS_DESK_SETTINGS");
        std::env::remove_var("DAIS_DESK_LEGACY_SETTINGS");

        let legacy_path = legacy_settings_path().expect("legacy path");
        fs::create_dir_all(legacy_path.parent().expect("legacy parent")).expect("legacy dir");
        fs::write(
            &legacy_path,
            r#"{
  "instance_url": "https://social.dais.social",
  "owner_token": "secret-token",
  "active_account_id": null,
  "accounts": []
}"#,
        )
        .expect("legacy settings");

        let desk_path = default_settings_path();
        assert!(!desk_path.exists());
        let settings = load_settings_from(&desk_path).expect("migrated settings");
        assert_eq!(settings.instance_url, DEFAULT_INSTANCE_URL);
        assert_eq!(settings.owner_token.as_deref(), Some("secret-token"));
        assert!(desk_path.exists());

        if let Some(value) = previous_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(value) = previous_settings {
            std::env::set_var("DAIS_DESK_SETTINGS", value);
        } else {
            std::env::remove_var("DAIS_DESK_SETTINGS");
        }
        if let Some(value) = previous_legacy {
            std::env::set_var("DAIS_DESK_LEGACY_SETTINGS", value);
        } else {
            std::env::remove_var("DAIS_DESK_LEGACY_SETTINGS");
        }
    }

    #[test]
    fn fixture_projection_has_all_primary_modes() {
        let controller = DeskController::fixture_for_tests();
        let projection = controller.projection();
        let modes: Vec<_> = projection
            .mode_nav
            .iter()
            .map(|item| item.id.to_string())
            .collect();
        assert_eq!(modes, vec!["home", "people", "server"]);
        assert!(projection
            .privacy_status
            .contains("Graph and watches are owner-only"));
    }
}
