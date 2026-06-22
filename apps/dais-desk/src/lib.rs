use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use dais_client_core::{
    ComposeDraft, DiagnosticStatus, ModerationReplyRow, ModerationSettingsUpdate, ModerationState,
    OwnerApiClient, OwnerAudienceList, OwnerAudienceListUpsert, OwnerCreatedPost, OwnerDeletedPost,
    OwnerDelivery, OwnerDirectMessage, OwnerDiscoveredActor, OwnerFollowResult, OwnerFollower,
    OwnerFollowing, OwnerFriend, OwnerInteraction, OwnerInteractionResult, OwnerMedia,
    OwnerMediaUpload, OwnerNotification, OwnerPost, OwnerPostDetail, OwnerProfile,
    OwnerProfileUpdate, OwnerPublicSearchActor, OwnerPublicSearchPost, OwnerSearchQuery,
    OwnerSearchResult, OwnerSection, OwnerSettings, OwnerSettingsUpdate, OwnerSourceAdd,
    OwnerSourceAddResult, OwnerSourceRefreshResult, OwnerSources, OwnerStats, OwnerTimelinePost,
    OwnerWatchAdd, ProtocolRoute, SourceItem, SourceSubscription, Visibility,
};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StoredDrafts {
    #[serde(default)]
    pub drafts: Vec<StoredDraft>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredDraft {
    pub id: String,
    pub account_id: String,
    pub text: String,
    pub visibility: Visibility,
    pub protocol: ProtocolRoute,
    pub encrypt: bool,
    pub in_reply_to: Option<String>,
    pub audience_list_id: Option<String>,
    pub recipients: String,
    pub media_description: String,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub updated_at: String,
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
    pub post_detail: Option<OwnerPostDetail>,
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
pub struct SearchFormState {
    pub scope: String,
    pub provider: String,
    pub result_type: String,
    pub servers: String,
    pub sort: String,
    pub since: String,
    pub until: String,
    pub author: String,
    pub mentions: String,
    pub lang: String,
    pub domain: String,
    pub url: String,
    pub tags: String,
    pub confirm_public_sensitive: bool,
}

impl Default for SearchFormState {
    fn default() -> Self {
        Self {
            scope: "public".into(),
            provider: "all".into(),
            result_type: "all".into(),
            servers: String::new(),
            sort: "recent".into(),
            since: String::new(),
            until: String::new(),
            author: String::new(),
            mentions: String::new(),
            lang: String::new(),
            domain: String::new(),
            url: String::new(),
            tags: String::new(),
            confirm_public_sensitive: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SourceFormState {
    pub source_type: String,
    pub url: String,
    pub title: String,
    pub cadence_minutes: String,
}

impl Default for SourceFormState {
    fn default() -> Self {
        Self {
            source_type: "rss".into(),
            url: String::new(),
            title: String::new(),
            cadence_minutes: "60".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WatchFormState {
    pub watch_type: String,
    pub target: String,
    pub title: String,
    pub cadence_minutes: String,
}

impl Default for WatchFormState {
    fn default() -> Self {
        Self {
            watch_type: "activitypub_actor".into(),
            target: String::new(),
            title: String::new(),
            cadence_minutes: "60".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProfileFormState {
    pub actor_type: String,
    pub display_name: String,
    pub summary: String,
    pub icon: String,
    pub image: String,
}

#[derive(Clone, Debug, Default)]
pub struct AudienceFormState {
    pub id: String,
    pub name: String,
    pub description: String,
    pub categories: String,
    pub members: String,
}

#[derive(Clone, Debug)]
pub struct ModerationFormState {
    pub reply_policy: String,
    pub ai_enabled: bool,
    pub ai_model: String,
    pub ai_daily_budget: String,
    pub block_actor: String,
    pub block_domain: String,
    pub block_reason: String,
    pub allow_host: String,
    pub allow_note: String,
}

impl Default for ModerationFormState {
    fn default() -> Self {
        Self {
            reply_policy: "warn".into(),
            ai_enabled: false,
            ai_model: String::new(),
            ai_daily_budget: "0".into(),
            block_actor: String::new(),
            block_domain: String::new(),
            block_reason: String::new(),
            allow_host: String::new(),
            allow_note: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsFormState {
    pub default_visibility: String,
    pub default_protocol: String,
    pub require_authorized_fetch: bool,
    pub manually_approves_followers: bool,
    pub closed_network: bool,
}

impl Default for SettingsFormState {
    fn default() -> Self {
        Self {
            default_visibility: "followers".into(),
            default_protocol: "activitypub".into(),
            require_authorized_fetch: true,
            manually_approves_followers: true,
            closed_network: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MediaFormState {
    pub file_path: String,
    pub media_type: String,
    pub description: String,
    pub access: String,
    pub expires_seconds: String,
    pub require_authorized_fetch: bool,
    pub revoke_url: String,
}

impl Default for MediaFormState {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            media_type: String::new(),
            description: String::new(),
            access: "followers".into(),
            expires_seconds: String::new(),
            require_authorized_fetch: true,
            revoke_url: String::new(),
        }
    }
}

pub struct SearchFormInput<'a> {
    pub query: &'a str,
    pub scope: &'a str,
    pub provider: &'a str,
    pub result_type: &'a str,
    pub servers: &'a str,
    pub sort: &'a str,
    pub since: &'a str,
    pub until: &'a str,
    pub author: &'a str,
    pub mentions: &'a str,
    pub lang: &'a str,
    pub domain: &'a str,
    pub url: &'a str,
    pub tags: &'a str,
    pub confirm_public_sensitive: bool,
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
    pub compose_audience_list: String,
    pub compose_media_description: String,
    pub compose_encrypt: bool,
    pub compose_visibility: String,
    pub compose_protocol: String,
    pub compose_warning: String,
    pub compose_audience_summary: String,
    pub compose_can_send: bool,
    pub account_label: String,
    pub account_url: String,
    pub account_token: String,
    pub search_scope: String,
    pub search_provider: String,
    pub search_type: String,
    pub search_servers: String,
    pub search_sort: String,
    pub search_since: String,
    pub search_until: String,
    pub search_author: String,
    pub search_mentions: String,
    pub search_lang: String,
    pub search_domain: String,
    pub search_url: String,
    pub search_tags: String,
    pub search_confirm_public_sensitive: bool,
    pub source_type: String,
    pub source_url: String,
    pub source_title: String,
    pub source_cadence: String,
    pub watch_type: String,
    pub watch_target: String,
    pub watch_title: String,
    pub watch_cadence: String,
    pub profile_actor_type: String,
    pub profile_display_name: String,
    pub profile_summary: String,
    pub profile_icon: String,
    pub profile_image: String,
    pub profile_preview: String,
    pub audience_id: String,
    pub audience_name: String,
    pub audience_description: String,
    pub audience_categories: String,
    pub audience_members: String,
    pub moderation_reply_policy: String,
    pub moderation_ai_enabled: bool,
    pub moderation_ai_model: String,
    pub moderation_ai_budget: String,
    pub moderation_block_actor: String,
    pub moderation_block_domain: String,
    pub moderation_block_reason: String,
    pub moderation_allow_host: String,
    pub moderation_allow_note: String,
    pub settings_default_visibility: String,
    pub settings_default_protocol: String,
    pub settings_require_authorized_fetch: bool,
    pub settings_manually_approves_followers: bool,
    pub settings_closed_network: bool,
    pub media_file_path: String,
    pub media_type: String,
    pub media_description: String,
    pub media_access: String,
    pub media_expires_seconds: String,
    pub media_authorized_fetch: bool,
    pub media_revoke_url: String,
}

pub struct DeskController {
    settings_path: PathBuf,
    drafts_path: PathBuf,
    settings: StoredOwnerSettings,
    drafts: StoredDrafts,
    runtime: tokio::runtime::Runtime,
    data: DeskData,
    active_mode: String,
    active_screen: String,
    selected_row: String,
    command_text: String,
    compose: ComposeState,
    search_form: SearchFormState,
    source_form: SourceFormState,
    watch_form: WatchFormState,
    profile_form: ProfileFormState,
    profile_preview_fingerprint: Option<String>,
    audience_form: AudienceFormState,
    moderation_form: ModerationFormState,
    settings_form: SettingsFormState,
    media_form: MediaFormState,
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
        let drafts_path = drafts_path_for_settings(&settings_path);
        let drafts = load_drafts_from(&drafts_path)?;
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
            drafts_path,
            settings,
            drafts,
            runtime,
            data: fixture_data(None),
            active_mode: "home".to_string(),
            active_screen: "today".to_string(),
            selected_row: String::new(),
            command_text: String::new(),
            compose: ComposeState::default(),
            search_form: SearchFormState::default(),
            source_form: SourceFormState::default(),
            watch_form: WatchFormState::default(),
            profile_form: ProfileFormState::default(),
            profile_preview_fingerprint: None,
            audience_form: AudienceFormState::default(),
            moderation_form: ModerationFormState::default(),
            settings_form: SettingsFormState::default(),
            media_form: MediaFormState::default(),
            status_message: "Ready.".to_string(),
            account_form_label,
            account_form_url,
            account_form_token,
        };
        controller.refresh();
        controller.sync_forms_from_data();
        Ok(controller)
    }

    pub fn fixture_for_tests() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let settings = StoredOwnerSettings::default();
        let mut controller = Self {
            settings_path: PathBuf::from("fixture-owner-settings.json"),
            drafts_path: PathBuf::from("fixture-owner-drafts.json"),
            settings,
            drafts: StoredDrafts::default(),
            runtime,
            data: fixture_data(None),
            active_mode: "home".to_string(),
            active_screen: "today".to_string(),
            selected_row: "post:fixture-private-post".to_string(),
            command_text: String::new(),
            compose: ComposeState::default(),
            search_form: SearchFormState::default(),
            source_form: SourceFormState::default(),
            watch_form: WatchFormState::default(),
            profile_form: ProfileFormState::default(),
            profile_preview_fingerprint: None,
            audience_form: AudienceFormState::default(),
            moderation_form: ModerationFormState::default(),
            settings_form: SettingsFormState::default(),
            media_form: MediaFormState::default(),
            status_message: "Fixture mode.".to_string(),
            account_form_label: "Dais Social".to_string(),
            account_form_url: DEFAULT_INSTANCE_URL.to_string(),
            account_form_token: String::new(),
        };
        controller.sync_forms_from_data();
        controller
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
        self.sync_forms_from_data();
    }

    fn sync_forms_from_data(&mut self) {
        let profile = &self.data.snapshot.profile;
        self.profile_form = ProfileFormState {
            actor_type: profile.actor_type.clone(),
            display_name: profile.display_name.clone().unwrap_or_default(),
            summary: profile.summary.clone().unwrap_or_default(),
            icon: profile
                .icon
                .clone()
                .or_else(|| profile.avatar_url.clone())
                .unwrap_or_default(),
            image: profile
                .image
                .clone()
                .or_else(|| profile.header_url.clone())
                .unwrap_or_default(),
        };

        let moderation = &self.data.snapshot.moderation;
        self.moderation_form.reply_policy = moderation.reply_policy.clone();
        self.moderation_form.ai_enabled = moderation.ai_enabled;
        self.moderation_form.ai_model = moderation.ai_model.clone().unwrap_or_default();
        self.moderation_form.ai_daily_budget = moderation.ai_daily_budget.to_string();
        self.settings_form.default_visibility =
            visibility_label(&self.data.snapshot.settings.default_visibility).to_ascii_lowercase();
        self.settings_form.default_protocol =
            protocol_label(&self.data.snapshot.settings.default_protocol).to_ascii_lowercase();
        self.settings_form.require_authorized_fetch = moderation.require_authorized_fetch;
        self.settings_form.manually_approves_followers = moderation.manually_approves_followers;
        self.settings_form.closed_network = moderation.closed_network;

        if self.audience_form.id.is_empty() {
            if let Some(list) = self.data.snapshot.audience_lists.first() {
                self.audience_form = audience_form_from_list(list);
            }
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
        self.populate_form_from_selected_row();
    }

    pub fn select_row(&mut self, row_id: &str) {
        self.selected_row = row_id.to_string();
        if let Some(object_id) = row_id.strip_prefix("post:") {
            self.compose.in_reply_to = None;
            self.status_message = match self.load_post_detail(object_id) {
                Ok(message) => message,
                Err(error) => {
                    format!("Selected post context {object_id}; detail unavailable: {error}")
                }
            };
        } else if let Some(object_id) = row_id.strip_prefix("timeline:") {
            self.status_message = match self.load_post_detail(object_id) {
                Ok(message) => message,
                Err(error) => {
                    format!("Selected timeline item {object_id}; detail unavailable: {error}")
                }
            };
        } else if let Some(actor) = row_id.strip_prefix("actor:") {
            self.status_message = format!("Selected relationship context for {actor}.");
            self.watch_form.target = actor.to_string();
        } else if let Some(id) = row_id.strip_prefix("audience:") {
            if let Some(list) = self
                .data
                .snapshot
                .audience_lists
                .iter()
                .find(|list| list.id == id)
            {
                self.audience_form = audience_form_from_list(list);
                self.status_message = format!("Editing audience group {}.", list.name);
            }
        } else if let Some(url) = row_id.strip_prefix("url:") {
            self.watch_form.target = url.to_string();
        } else if row_id.starts_with("draft:") {
            self.status_message = "Selected local draft. Open it to continue editing.".into();
        }
    }

    pub fn select_first_row(&mut self) {
        let rows = self.rows_for_active_screen();
        if let Some(first) = rows.first() {
            self.selected_row = first.id.to_string();
            self.populate_form_from_selected_row();
        } else {
            self.selected_row.clear();
        }
    }

    pub fn select_last_row(&mut self) {
        let rows = self.rows_for_active_screen();
        if let Some(last) = rows.last() {
            self.selected_row = last.id.to_string();
            self.populate_form_from_selected_row();
        } else {
            self.selected_row.clear();
        }
    }

    fn row_ids_for_active_screen(&self) -> Vec<String> {
        self.rows_for_active_screen()
            .into_iter()
            .map(|row| row.id.to_string())
            .collect()
    }

    pub fn move_row_selection(&mut self, delta: isize) {
        let row_ids = self.row_ids_for_active_screen();
        if row_ids.is_empty() {
            self.selected_row.clear();
            return;
        }

        let start_index = row_ids
            .iter()
            .position(|id| id == &self.selected_row)
            .unwrap_or(0) as isize;
        let last_index = row_ids.len() as isize - 1;
        let mut target = start_index + delta;
        if target < 0 {
            target = 0;
        }
        if target > last_index {
            target = last_index;
        }
        self.selected_row = row_ids[target as usize].to_string();
        self.populate_form_from_selected_row();
    }

    pub fn move_row_selection_next(&mut self) {
        self.move_row_selection(1);
    }

    pub fn move_row_selection_previous(&mut self) {
        self.move_row_selection(-1);
    }

    pub fn set_row_match_from_prefix(&mut self, prefix: &str) {
        let query = prefix.trim().to_lowercase();
        if query.is_empty() {
            return;
        }
        let match_row = self.rows_for_active_screen().into_iter().find(|row| {
            row.title.to_lowercase().starts_with(&query)
                || row.subtitle.to_lowercase().starts_with(&query)
                || row.id.to_lowercase().starts_with(&query)
        });

        if let Some(row) = match_row {
            self.selected_row = row.id.to_string();
            self.populate_form_from_selected_row();
        }
    }

    pub fn execute_selected_row_default_action(&mut self) {
        let rows = self.rows_for_active_screen();
        if rows.is_empty() {
            return;
        }
        let selected = if self.selected_row.is_empty() {
            rows.first().map(|row| row.id.to_string())
        } else {
            Some(self.selected_row.clone())
        };
        if selected.is_none() {
            return;
        }
        let selected = selected.unwrap_or_default();
        let row = rows.iter().find(|row| row.id.as_str() == selected.as_str());
        if let Some(row) = row {
            let action = if !row.primary.is_empty() {
                row.primary.to_string()
            } else {
                row.secondary.to_string()
            };
            if !action.is_empty() {
                self.row_action(&selected, action.as_str());
            }
        }
    }

    fn populate_form_from_selected_row(&mut self) {
        if let Some(id) = self.selected_row.strip_prefix("audience:") {
            if let Some(list) = self
                .data
                .snapshot
                .audience_lists
                .iter()
                .find(|list| list.id == id)
            {
                self.audience_form = audience_form_from_list(list);
            }
        } else if let Some(target) = self.selected_row.strip_prefix("actor:") {
            self.watch_form.target = target.to_string();
        } else if let Some(target) = self.selected_row.strip_prefix("url:") {
            self.watch_form.target = target.to_string();
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
        let result = if action == "Switch" && row_id.starts_with("account:") {
            self.switch_account_result(row_id.trim_start_matches("account:"))
        } else if action == "Delete" && row_id.starts_with("account:") {
            self.delete_account_result(row_id.trim_start_matches("account:"))
        } else if action == "Validate token" && row_id.starts_with("account:") {
            self.validate_account_token(row_id.trim_start_matches("account:"))
        } else if action == "Delete" && row_id.starts_with("audience:") {
            self.delete_audience(row_id.trim_start_matches("audience:"))
        } else if action == "Remove" && row_id.starts_with("audience:") {
            self.delete_audience(row_id.trim_start_matches("audience:"))
        } else if action == "Remove" && row_id.starts_with("allow:") {
            self.disallow_host(row_id.trim_start_matches("allow:"))
        } else if action == "Revoke media" {
            self.revoke_media_from_row(row_id)
        } else if action == "Use in compose" && row_id.starts_with("audience:") {
            self.use_audience_in_compose(row_id.trim_start_matches("audience:"))
        } else if action == "Save draft" {
            self.save_current_draft_inner()
        } else if action == "Open draft" && row_id.starts_with("draft:") {
            self.open_draft(row_id.trim_start_matches("draft:"))
        } else if action == "Delete draft" && row_id.starts_with("draft:") {
            self.delete_draft(row_id.trim_start_matches("draft:"))
        } else {
            match action {
                "Reply" => self.prepare_reply(row_id),
                "Favorite" => self.interact(row_id, "favorite"),
                "Boost" | "Repost" => self.interact(row_id, "boost"),
                "Delete" => self.delete_post(row_id),
                "Mark read" => self.mark_notification_read(row_id),
                "Approve" => self.set_follower_status(row_id, "approved"),
                "Reject" => self.set_follower_status(row_id, "rejected"),
                "Remove" => self.set_follower_status(row_id, "removed"),
                "Follow" => self.follow(row_id),
                "Unfollow" | "Cancel" | "Unfriend" => self.unfollow(row_id),
                "Watch" => self.watch(row_id),
                "Stop watching" => self.remove_source_or_watch(row_id),
                "Refresh" => self.refresh_row(row_id),
                "Retry delivery" => self.retry_delivery(row_id),
                "Cancel delivery" => self.cancel_delivery(row_id),
                "Approve reply" => self.set_reply_status(row_id, "approved"),
                "Hide reply" => self.set_reply_status(row_id, "hidden"),
                "Reject reply" => self.set_reply_status(row_id, "rejected"),
                "Block" => self.block(row_id),
                "Unblock" => self.unblock(row_id),
                "Open original" | "Open link" => self.open_external(row_id),
                "Find people" => {
                    self.active_mode = "people".to_string();
                    self.active_screen = "find".to_string();
                    self.selected_row = self.first_row_id();
                    Ok(
                        "Opened Find. Paste a handle, URL, feed, domain, or public search."
                            .to_string(),
                    )
                }
                "Add Watch" => {
                    self.active_mode = "people".to_string();
                    self.active_screen = "watches".to_string();
                    self.selected_row = self.first_row_id();
                    Ok(
                        "Opened Watches & Sources. Add a public account watch or RSS/Atom source."
                            .to_string(),
                    )
                }
                "Open context" => {
                    if let Some(context_row) = self.context_row_for(row_id) {
                        self.selected_row = context_row.clone();
                        if let Some(object_id) = object_id_from_row(&context_row) {
                            let _ = self.load_post_detail(object_id);
                        }
                        Ok("Opened related context.".to_string())
                    } else {
                        Ok("No related post context is available for this item.".to_string())
                    }
                }
                "Inspect delivery" => {
                    self.active_mode = "server".to_string();
                    self.active_screen = "deliveries".to_string();
                    self.selected_row = if row_id.starts_with("delivery:") {
                        row_id.to_string()
                    } else {
                        self.data
                            .deliveries
                            .iter()
                            .find(|delivery| delivery.status == "failed")
                            .or_else(|| self.data.deliveries.first())
                            .map(|delivery| format!("delivery:{}", delivery.id))
                            .unwrap_or_else(|| row_id.to_string())
                    };
                    Ok("Opened delivery inspector.".to_string())
                }
                "Copy evidence" => {
                    if let Some(row) = self.find_row(row_id) {
                        let evidence =
                            if row.id.starts_with("health:") || row.id.starts_with("diagnostic:") {
                                let detail = if row.detail.is_empty() {
                                    "open diagnostics on server for raw evidence"
                                } else {
                                    row.detail.as_str()
                                };
                                format!("Evidence: {} — {}", row.subtitle, detail)
                            } else if let Some(delivery_id) = row.id.strip_prefix("delivery:") {
                                self.data
                                    .deliveries
                                    .iter()
                                    .find(|delivery| delivery.id == delivery_id)
                                    .map(|delivery| {
                                        format!(
                                            "Delivery evidence: {} {} (status: {}, activity: {})",
                                            compact_url(&delivery.target_url),
                                            delivery.error_message.clone().unwrap_or_default(),
                                            delivery.status,
                                            delivery.activity_type.as_deref().unwrap_or("Unknown")
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        "Open delivery list on server for raw evidence".to_string()
                                    })
                            } else {
                                format!("Evidence: {}", row.title)
                            };
                        Ok(evidence)
                    } else {
                        Ok("No evidence target was found for this row.".to_string())
                    }
                }
                _ => Ok(format!(
                    "{action} is visible but not destructive in preview mode."
                )),
            }
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
                | "Switch"
                | "Mark read"
                | "Approve"
                | "Reject"
                | "Remove"
                | "Follow"
                | "Unfollow"
                | "Cancel"
                | "Stop watching"
                | "Refresh"
                | "Retry delivery"
                | "Cancel delivery"
                | "Approve reply"
                | "Hide reply"
                | "Reject reply"
                | "Block"
                | "Unblock"
                | "Revoke media"
                | "Delete draft"
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
        match self.switch_account_result(account_id) {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Switch failed: {error}"),
        }
    }

    pub fn delete_account(&mut self, account_id: &str) {
        match self.delete_account_result(account_id) {
            Ok(message) => {
                self.status_message = message;
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
        audience_list_id: &str,
        media_description: &str,
        encrypt: bool,
    ) {
        self.compose.text = text.to_string();
        self.compose.recipients = recipients.to_string();
        self.compose.audience_list_id = optional_trimmed(audience_list_id);
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
                self.compose.audience_list_id = None;
                self.compose.attachments.clear();
                self.active_screen = "today".into();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Post failed: {error}"),
        }
    }

    pub fn save_current_draft(&mut self) {
        match self.save_current_draft_inner() {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("Save draft failed: {error}"),
        }
    }

    pub fn run_filtered_search(&mut self, input: SearchFormInput<'_>) {
        self.search_form = SearchFormState {
            scope: input.scope.trim().if_empty("public"),
            provider: input.provider.trim().if_empty("all"),
            result_type: input.result_type.trim().if_empty("all"),
            servers: input.servers.trim().to_string(),
            sort: input.sort.trim().if_empty("recent"),
            since: input.since.trim().to_string(),
            until: input.until.trim().to_string(),
            author: input.author.trim().to_string(),
            mentions: input.mentions.trim().to_string(),
            lang: input.lang.trim().to_string(),
            domain: input.domain.trim().to_string(),
            url: input.url.trim().to_string(),
            tags: input.tags.trim().to_string(),
            confirm_public_sensitive: input.confirm_public_sensitive,
        };
        self.command_text = input.query.trim().to_string();
        self.active_mode = "people".into();
        self.active_screen = "find".into();
        match self.filtered_search_inner() {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("Search failed: {error}"),
        }
        self.selected_row = self.first_row_id();
    }

    pub fn add_source_from_form(
        &mut self,
        source_type: &str,
        url: &str,
        title: &str,
        cadence: &str,
    ) {
        self.source_form = SourceFormState {
            source_type: source_type.trim().if_empty("rss"),
            url: url.trim().to_string(),
            title: title.trim().to_string(),
            cadence_minutes: cadence.trim().if_empty("60"),
        };
        match self.add_source_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Add source failed: {error}"),
        }
    }

    pub fn add_watch_from_form(
        &mut self,
        watch_type: &str,
        target: &str,
        title: &str,
        cadence: &str,
    ) {
        self.watch_form = WatchFormState {
            watch_type: watch_type.trim().if_empty("activitypub_actor"),
            target: target.trim().to_string(),
            title: title.trim().to_string(),
            cadence_minutes: cadence.trim().if_empty("60"),
        };
        match self.add_watch_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Add watch failed: {error}"),
        }
    }

    pub fn save_profile_from_form(
        &mut self,
        actor_type: &str,
        display_name: &str,
        summary: &str,
        icon: &str,
        image: &str,
    ) {
        self.set_profile_form(actor_type, display_name, summary, icon, image);
        match self.save_profile_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Profile save failed: {error}"),
        }
    }

    pub fn preview_profile_from_form(
        &mut self,
        actor_type: &str,
        display_name: &str,
        summary: &str,
        icon: &str,
        image: &str,
    ) {
        self.set_profile_form(actor_type, display_name, summary, icon, image);
        self.profile_preview_fingerprint = Some(profile_form_fingerprint(&self.profile_form));
        self.status_message =
            "Reviewed public profile preview. Save profile will publish these exact values.".into();
    }

    fn set_profile_form(
        &mut self,
        actor_type: &str,
        display_name: &str,
        summary: &str,
        icon: &str,
        image: &str,
    ) {
        self.profile_form = ProfileFormState {
            actor_type: actor_type.trim().to_string(),
            display_name: display_name.trim().to_string(),
            summary: summary.trim().to_string(),
            icon: icon.trim().to_string(),
            image: image.trim().to_string(),
        };
    }

    pub fn save_audience_from_form(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        categories: &str,
        members: &str,
    ) {
        self.audience_form = AudienceFormState {
            id: id.trim().to_string(),
            name: name.trim().to_string(),
            description: description.trim().to_string(),
            categories: categories.trim().to_string(),
            members: members.trim().to_string(),
        };
        match self.save_audience_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Audience save failed: {error}"),
        }
    }

    pub fn delete_audience_from_form(&mut self, id: &str) {
        match self.delete_audience(id.trim()) {
            Ok(message) => {
                self.status_message = message;
                self.audience_form = AudienceFormState::default();
                self.refresh();
            }
            Err(error) => self.status_message = format!("Audience delete failed: {error}"),
        }
    }

    pub fn save_moderation_from_form(
        &mut self,
        reply_policy: &str,
        ai_enabled: bool,
        ai_model: &str,
        ai_budget: &str,
    ) {
        self.moderation_form.reply_policy = reply_policy.trim().if_empty("warn");
        self.moderation_form.ai_enabled = ai_enabled;
        self.moderation_form.ai_model = ai_model.trim().to_string();
        self.moderation_form.ai_daily_budget = ai_budget.trim().if_empty("0");
        match self.save_moderation_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Moderation save failed: {error}"),
        }
    }

    pub fn save_settings_from_form(
        &mut self,
        default_visibility: &str,
        default_protocol: &str,
        require_authorized_fetch: bool,
        manually_approves_followers: bool,
        closed_network: bool,
    ) {
        self.settings_form = SettingsFormState {
            default_visibility: default_visibility.trim().if_empty("followers"),
            default_protocol: default_protocol.trim().if_empty("activitypub"),
            require_authorized_fetch,
            manually_approves_followers,
            closed_network,
        };
        match self.save_settings_inner() {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Settings save failed: {error}"),
        }
    }

    pub fn block_actor_from_form(&mut self, actor_id: &str, reason: &str) {
        self.moderation_form.block_actor = actor_id.trim().to_string();
        self.moderation_form.block_reason = reason.trim().to_string();
        match self.block_actor_value(actor_id.trim(), reason.trim()) {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Block actor failed: {error}"),
        }
    }

    pub fn block_domain_from_form(&mut self, domain: &str, reason: &str) {
        self.moderation_form.block_domain = domain.trim().to_string();
        self.moderation_form.block_reason = reason.trim().to_string();
        match self.block_domain_value(domain.trim(), reason.trim()) {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Block domain failed: {error}"),
        }
    }

    pub fn allow_host_from_form(&mut self, host: &str, note: &str) {
        self.moderation_form.allow_host = host.trim().to_string();
        self.moderation_form.allow_note = note.trim().to_string();
        match self.allow_host(host.trim(), note.trim()) {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Allow host failed: {error}"),
        }
    }

    pub fn disallow_host_from_form(&mut self, host: &str) {
        match self.disallow_host(host.trim()) {
            Ok(message) => {
                self.status_message = message;
                self.refresh();
            }
            Err(error) => self.status_message = format!("Remove host failed: {error}"),
        }
    }

    pub fn upload_media_from_form(
        &mut self,
        file_path: &str,
        media_type: &str,
        description: &str,
        access: &str,
        expires_seconds: &str,
        require_authorized_fetch: bool,
    ) {
        self.media_form.file_path = file_path.trim().to_string();
        self.media_form.media_type = media_type.trim().to_string();
        self.media_form.description = description.trim().to_string();
        self.media_form.access = access.trim().if_empty("followers");
        self.media_form.expires_seconds = expires_seconds.trim().to_string();
        self.media_form.require_authorized_fetch = require_authorized_fetch;
        match self.upload_media_inner() {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("Media upload failed: {error}"),
        }
    }

    pub fn choose_media_file(&mut self) {
        match choose_media_file_path() {
            Ok(Some(path)) => {
                self.set_media_file_path(&path);
                self.status_message = "Selected local media file.".into();
            }
            Ok(None) => {
                self.status_message = "Media selection cancelled.".into();
            }
            Err(error) => {
                self.status_message = format!("Choose media failed: {error}");
            }
        }
    }

    pub fn set_media_file_path(&mut self, path: &str) {
        self.media_form.file_path = path.trim().to_string();
        if self.media_form.media_type.trim().is_empty() {
            self.media_form.media_type = media_type_for_path(Path::new(&self.media_form.file_path));
        }
    }

    pub fn revoke_media_from_form(&mut self, url: &str) {
        self.media_form.revoke_url = url.trim().to_string();
        match self.revoke_media_url(url.trim()) {
            Ok(message) => self.status_message = message,
            Err(error) => self.status_message = format!("Media revoke failed: {error}"),
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
                .map(|account| account_row(account, self.settings.accounts.len() > 1))
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
            compose_audience_list: self.compose.audience_list_id.clone().unwrap_or_default(),
            compose_media_description: self.compose.media_description.clone(),
            compose_encrypt: self.compose.encrypt,
            compose_visibility: visibility_label(&self.compose.visibility).to_lowercase(),
            compose_protocol: protocol_label(&self.compose.protocol).to_lowercase(),
            compose_can_send: compose_can_send(&self.compose),
            compose_warning,
            compose_audience_summary: compose_audience_summary(&self.compose, &self.data.snapshot),
            account_label: self
                .account_form_label
                .clone()
                .if_empty_else(|| account.map(|a| a.label.clone()).unwrap_or_default()),
            account_url: self
                .account_form_url
                .clone()
                .if_empty_else(|| account.map(|a| a.instance_url.clone()).unwrap_or_default()),
            account_token: self.account_form_token.clone(),
            search_scope: self.search_form.scope.clone(),
            search_provider: self.search_form.provider.clone(),
            search_type: self.search_form.result_type.clone(),
            search_servers: self.search_form.servers.clone(),
            search_sort: self.search_form.sort.clone(),
            search_since: self.search_form.since.clone(),
            search_until: self.search_form.until.clone(),
            search_author: self.search_form.author.clone(),
            search_mentions: self.search_form.mentions.clone(),
            search_lang: self.search_form.lang.clone(),
            search_domain: self.search_form.domain.clone(),
            search_url: self.search_form.url.clone(),
            search_tags: self.search_form.tags.clone(),
            search_confirm_public_sensitive: self.search_form.confirm_public_sensitive,
            source_type: self.source_form.source_type.clone(),
            source_url: self.source_form.url.clone(),
            source_title: self.source_form.title.clone(),
            source_cadence: self.source_form.cadence_minutes.clone(),
            watch_type: self.watch_form.watch_type.clone(),
            watch_target: self.watch_form.target.clone(),
            watch_title: self.watch_form.title.clone(),
            watch_cadence: self.watch_form.cadence_minutes.clone(),
            profile_actor_type: self.profile_form.actor_type.clone(),
            profile_display_name: self.profile_form.display_name.clone(),
            profile_summary: self.profile_form.summary.clone(),
            profile_icon: self.profile_form.icon.clone(),
            profile_image: self.profile_form.image.clone(),
            profile_preview: profile_preview_text(&self.profile_form),
            audience_id: self.audience_form.id.clone(),
            audience_name: self.audience_form.name.clone(),
            audience_description: self.audience_form.description.clone(),
            audience_categories: self.audience_form.categories.clone(),
            audience_members: self.audience_form.members.clone(),
            moderation_reply_policy: self.moderation_form.reply_policy.clone(),
            moderation_ai_enabled: self.moderation_form.ai_enabled,
            moderation_ai_model: self.moderation_form.ai_model.clone(),
            moderation_ai_budget: self.moderation_form.ai_daily_budget.clone(),
            moderation_block_actor: self.moderation_form.block_actor.clone(),
            moderation_block_domain: self.moderation_form.block_domain.clone(),
            moderation_block_reason: self.moderation_form.block_reason.clone(),
            moderation_allow_host: self.moderation_form.allow_host.clone(),
            moderation_allow_note: self.moderation_form.allow_note.clone(),
            settings_default_visibility: self.settings_form.default_visibility.clone(),
            settings_default_protocol: self.settings_form.default_protocol.clone(),
            settings_require_authorized_fetch: self.settings_form.require_authorized_fetch,
            settings_manually_approves_followers: self.settings_form.manually_approves_followers,
            settings_closed_network: self.settings_form.closed_network,
            media_file_path: self.media_form.file_path.clone(),
            media_type: self.media_form.media_type.clone(),
            media_description: self.media_form.description.clone(),
            media_access: self.media_form.access.clone(),
            media_expires_seconds: self.media_form.expires_seconds.clone(),
            media_authorized_fetch: self.media_form.require_authorized_fetch,
            media_revoke_url: self.media_form.revoke_url.clone(),
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
                post_detail: None,
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

    fn load_post_detail(&mut self, object_id: &str) -> Result<String, String> {
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            self.data.post_detail = fixture_post_detail(object_id, &self.data.snapshot);
            return Ok("Preview post detail loaded in the inspector.".into());
        }
        let client = self.client()?;
        let id = object_id.to_string();
        let detail = self.runtime.block_on(async move {
            client
                .post_detail(&id)
                .await
                .map_err(|error| error.to_string())
        })?;
        let reply_count = detail.replies.len();
        let like_count = detail.likes.len();
        let boost_count = detail.boosts.len();
        self.data.post_detail = Some(detail);
        Ok(format!(
            "Loaded post detail: {reply_count} replies, {like_count} likes, {boost_count} boosts."
        ))
    }

    fn filtered_search_inner(&mut self) -> Result<String, String> {
        if self.command_text.trim().is_empty() {
            return Err("search text is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            self.data.search = fixture_search(&self.command_text);
            return Ok("Preview filtered search results are shown.".into());
        }
        let client = self.client()?;
        let options = OwnerSearchQuery {
            query: self.command_text.trim().to_string(),
            scope: self.search_form.scope.trim().if_empty("public"),
            confirm_public_sensitive: self.search_form.confirm_public_sensitive,
            provider: optional_filter(&self.search_form.provider, "all"),
            result_type: optional_filter(&self.search_form.result_type, "all"),
            servers: split_list(&self.search_form.servers),
            sort: optional_filter(&self.search_form.sort, ""),
            since: optional_trimmed(&self.search_form.since),
            until: optional_trimmed(&self.search_form.until),
            author: optional_trimmed(&self.search_form.author),
            mentions: optional_trimmed(&self.search_form.mentions),
            lang: optional_trimmed(&self.search_form.lang),
            domain: optional_trimmed(&self.search_form.domain),
            url: optional_trimmed(&self.search_form.url),
            tags: split_list(&self.search_form.tags),
        };
        let result = self.runtime.block_on(async move {
            client
                .search_with_options(&options)
                .await
                .map_err(|error| error.to_string())
        })?;
        let guard = result.public_search_guard.clone();
        let result_count = result.public_posts.len()
            + result.public_actors.len()
            + result.posts.len()
            + result.users.len()
            + result.sources.len()
            + result.source_items.len();
        self.data.search = result;
        if guard.requires_confirmation && !guard.confirmed {
            Ok(guard.message.unwrap_or_else(|| {
                "Public search looks sensitive. Enable confirmation and search again.".into()
            }))
        } else {
            Ok(format!("Loaded {result_count} search result(s)."))
        }
    }

    fn add_source_inner(&self) -> Result<String, String> {
        if self.source_form.url.trim().is_empty() {
            return Err("source URL is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview source added. Add an owner token to save it.".into());
        }
        let client = self.client()?;
        let source = OwnerSourceAdd {
            source_type: self.source_form.source_type.trim().if_empty("rss"),
            url: self.source_form.url.trim().to_string(),
            title: optional_trimmed(&self.source_form.title),
            cadence_minutes: parse_u16(&self.source_form.cadence_minutes, Some(60)),
            api_secret_name: None,
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            image_allowed: false,
            full_text_allowed: false,
        };
        let result: OwnerSourceAddResult = self.runtime.block_on(async move {
            client
                .add_source(&source)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Added {} source {}.",
            result.source.source_type, result.source.url
        ))
    }

    fn add_watch_inner(&self) -> Result<String, String> {
        if self.watch_form.target.trim().is_empty() {
            return Err("watch target is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview watch added. No remote follow request is sent.".into());
        }
        let client = self.client()?;
        let watch = OwnerWatchAdd {
            watch_type: self
                .watch_form
                .watch_type
                .trim()
                .if_empty("activitypub_actor"),
            target: self.watch_form.target.trim().to_string(),
            title: optional_trimmed(&self.watch_form.title),
            cadence_minutes: parse_u16(&self.watch_form.cadence_minutes, Some(60)),
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            image_allowed: false,
            full_text_allowed: false,
        };
        let result: OwnerSourceAddResult = self.runtime.block_on(async move {
            client
                .add_watch(&watch)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Added private {} watch for {}.",
            result.source.source_type, result.source.url
        ))
    }

    fn save_profile_inner(&self) -> Result<String, String> {
        let current_fingerprint = profile_form_fingerprint(&self.profile_form);
        if self.profile_preview_fingerprint.as_deref() != Some(current_fingerprint.as_str()) {
            return Err(
                "preview the public identity first; changed fields require a fresh preview".into(),
            );
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview profile saved. Add an owner token to update the server.".into());
        }
        let client = self.client()?;
        let profile = OwnerProfileUpdate {
            actor_type: optional_trimmed(&self.profile_form.actor_type),
            display_name: optional_trimmed(&self.profile_form.display_name),
            summary: optional_trimmed(&self.profile_form.summary),
            icon: optional_trimmed(&self.profile_form.icon),
            image: optional_trimmed(&self.profile_form.image),
        };
        let result: OwnerProfile = self.runtime.block_on(async move {
            client
                .update_profile(&profile)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!("Updated public profile {}.", result.public_handle))
    }

    fn save_audience_inner(&self) -> Result<String, String> {
        if self.audience_form.name.trim().is_empty() {
            return Err("audience name is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview audience group saved.".into());
        }
        let client = self.client()?;
        let list = OwnerAudienceListUpsert {
            id: optional_trimmed(&self.audience_form.id),
            name: self.audience_form.name.trim().to_string(),
            description: optional_trimmed(&self.audience_form.description),
            allowed_categories: split_list(&self.audience_form.categories),
            member_actor_ids: split_list(&self.audience_form.members),
        };
        let result: OwnerAudienceList = self.runtime.block_on(async move {
            client
                .upsert_audience_list(&list)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Saved audience group {} with {} member(s).",
            result.name, result.member_count
        ))
    }

    fn delete_audience(&self, id: &str) -> Result<String, String> {
        if id.trim().is_empty() {
            return Err("audience id is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview audience group deleted.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .delete_audience_list(id)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Audience group deleted.".into())
    }

    fn save_moderation_inner(&self) -> Result<String, String> {
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview moderation policy saved.".into());
        }
        let client = self.client()?;
        let settings = ModerationSettingsUpdate {
            reply_policy: self.moderation_form.reply_policy.trim().if_empty("warn"),
            ai_enabled: self.moderation_form.ai_enabled,
            ai_model: optional_trimmed(&self.moderation_form.ai_model),
            ai_daily_budget: parse_u64(&self.moderation_form.ai_daily_budget, 0),
        };
        let result: ModerationState = self.runtime.block_on(async move {
            client
                .update_moderation_settings(&settings)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Updated moderation policy {}. AI advisory is {}.",
            result.reply_policy,
            if result.ai_enabled { "on" } else { "off" }
        ))
    }

    fn save_settings_inner(&self) -> Result<String, String> {
        let default_visibility = visibility_from_value(&self.settings_form.default_visibility)
            .ok_or_else(|| "unsupported default visibility".to_string())?;
        let default_protocol = protocol_from_value(&self.settings_form.default_protocol)
            .ok_or_else(|| "unsupported default protocol".to_string())?;
        if matches!(
            default_visibility,
            Visibility::Followers | Visibility::Direct
        ) && matches!(default_protocol, ProtocolRoute::AtProto)
        {
            return Err("private defaults cannot route only to Bluesky".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview owner settings saved.".into());
        }
        let client = self.client()?;
        let settings = OwnerSettingsUpdate {
            default_visibility,
            default_protocol,
            require_authorized_fetch: self.settings_form.require_authorized_fetch,
            manually_approves_followers: self.settings_form.manually_approves_followers,
            closed_network: self.settings_form.closed_network,
        };
        let result: OwnerSettings = self.runtime.block_on(async move {
            client
                .update_settings(&settings)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Updated defaults to {} via {}.",
            visibility_label(&result.default_visibility),
            protocol_label(&result.default_protocol)
        ))
    }

    fn block_actor_value(&self, actor_id: &str, reason: &str) -> Result<String, String> {
        if actor_id.is_empty() {
            return Err("actor id is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview actor block recorded.".into());
        }
        let client = self.client()?;
        let reason = optional_trimmed(reason);
        self.runtime.block_on(async move {
            client
                .block_actor(actor_id, reason.as_deref())
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Actor blocked.".into())
    }

    fn block_domain_value(&self, domain: &str, reason: &str) -> Result<String, String> {
        if domain.is_empty() {
            return Err("domain is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview domain block recorded.".into());
        }
        let client = self.client()?;
        let reason = optional_trimmed(reason);
        self.runtime.block_on(async move {
            client
                .block_domain(domain, reason.as_deref())
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Domain blocked.".into())
    }

    fn allow_host(&self, host: &str, note: &str) -> Result<String, String> {
        if host.is_empty() {
            return Err("host is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview allowlist host saved.".into());
        }
        let client = self.client()?;
        let note = optional_trimmed(note);
        self.runtime.block_on(async move {
            client
                .allow_host(host, note.as_deref())
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Allowlist host saved.".into())
    }

    fn disallow_host(&self, host: &str) -> Result<String, String> {
        if host.is_empty() {
            return Err("host is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview allowlist host removed.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .disallow_host(host)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok("Allowlist host removed.".into())
    }

    fn upload_media_inner(&mut self) -> Result<String, String> {
        if self.media_form.file_path.trim().is_empty() {
            return Err("local media file path is required".into());
        }
        let path = PathBuf::from(self.media_form.file_path.trim());
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "media filename is required".to_string())?
            .to_string();
        let media_type = optional_trimmed(&self.media_form.media_type)
            .unwrap_or_else(|| media_type_for_path(&path));
        let expires_in_seconds = optional_trimmed(&self.media_form.expires_seconds)
            .map(|value| parse_u64(&value, 0))
            .filter(|value| *value > 0);
        let upload = OwnerMediaUpload {
            filename,
            media_type: Some(media_type),
            description: optional_trimmed(&self.media_form.description),
            access: optional_trimmed(&self.media_form.access),
            expires_in_seconds,
            require_authorized_fetch: Some(self.media_form.require_authorized_fetch),
            data_base64: BASE64.encode(bytes),
        };
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok(format!(
                "Preview media upload prepared for {}.",
                upload.filename
            ));
        }
        let client = self.client()?;
        let result: OwnerMedia = self.runtime.block_on(async move {
            client
                .upload_media(&upload)
                .await
                .map_err(|error| error.to_string())
        })?;
        self.compose.attachments.push(result.url.clone());
        self.media_form.revoke_url = result.url.clone();
        Ok(format!(
            "Uploaded media and attached it to the current draft: {}.",
            compact_url(&result.url)
        ))
    }

    fn revoke_media_from_row(&mut self, row_id: &str) -> Result<String, String> {
        let url = row_id
            .strip_prefix("media:")
            .or_else(|| row_id.strip_prefix("url:"))
            .ok_or_else(|| "no media URL".to_string())?;
        self.revoke_media_url(url)
    }

    fn revoke_media_url(&mut self, url: &str) -> Result<String, String> {
        if url.is_empty() {
            return Err("media URL is required".into());
        }
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            self.compose.attachments.retain(|item| item != url);
            return Ok("Preview media revoked.".into());
        }
        let client = self.client()?;
        self.runtime.block_on(async move {
            client
                .revoke_media(url)
                .await
                .map_err(|error| error.to_string())
        })?;
        self.compose.attachments.retain(|item| item != url);
        Ok("Media revoked and removed from the current draft.".into())
    }

    fn switch_account_result(&mut self, account_id: &str) -> Result<String, String> {
        if !self
            .settings
            .accounts
            .iter()
            .any(|account| account.id == account_id)
        {
            return Err("account not found".into());
        }
        self.settings.active_account_id = Some(account_id.to_string());
        persist_settings_to(
            &self.settings_path,
            normalize_settings(self.settings.clone()),
        )?;
        self.settings = load_settings_from(&self.settings_path).unwrap_or_default();
        Ok(
            "Switched account. Reads, posts, follows, watches, and server commands use it now."
                .into(),
        )
    }

    fn delete_account_result(&mut self, account_id: &str) -> Result<String, String> {
        if self.settings.accounts.len() <= 1 {
            return Err("at least one account profile is required".into());
        }
        let before = self.settings.accounts.len();
        self.settings
            .accounts
            .retain(|account| account.id != account_id);
        if before == self.settings.accounts.len() {
            return Err("account not found".into());
        }
        if self.settings.active_account_id.as_deref() == Some(account_id) {
            self.settings.active_account_id = self.settings.accounts.first().map(|a| a.id.clone());
        }
        persist_settings_to(
            &self.settings_path,
            normalize_settings(self.settings.clone()),
        )?;
        self.settings = load_settings_from(&self.settings_path).unwrap_or_default();
        Ok("Deleted account profile.".into())
    }

    fn validate_account_token(&self, account_id: &str) -> Result<String, String> {
        let account = self
            .settings
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| "account not found".to_string())?;
        let token = account
            .owner_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "owner token is required for validation".to_string())?;
        let client = OwnerApiClient::new(&account.instance_url, token);
        let diagnostics = self.runtime.block_on(async move {
            client
                .diagnostics()
                .await
                .map_err(|error| error.to_string())
        })?;
        let failing = diagnostics.iter().filter(|item| !item.ok).count();
        Ok(format!(
            "Validated {}. {} diagnostic check(s), {} need attention.",
            account.label,
            diagnostics.len(),
            failing
        ))
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
        if let Some(id) = row_id.strip_prefix("dm:") {
            let dm = self
                .data
                .direct_messages
                .iter()
                .find(|dm| dm.id == id)
                .ok_or_else(|| "direct message not found".to_string())?;
            self.compose.in_reply_to = None;
            self.compose.visibility = Visibility::Direct;
            self.compose.protocol = ProtocolRoute::ActivityPub;
            self.compose.recipients = dm.sender_id.clone();
            self.active_mode = "home".to_string();
            self.active_screen = "compose".to_string();
            return Ok("Direct reply prepared. Recipient and Direct visibility are visible before sending.".into());
        }
        if let Some(id) = notification_id_from_row(row_id) {
            let notice = self
                .data
                .notifications
                .iter()
                .find(|notice| notice.id == id)
                .ok_or_else(|| "notification not found".to_string())?;
            let object_id = notice
                .context_post_id
                .as_deref()
                .or(notice.post_id.as_deref())
                .ok_or_else(|| "notification has no post context".to_string())?;
            self.compose.in_reply_to = Some(object_id.to_string());
            self.compose.visibility = match notice.context_post_visibility.as_deref() {
                Some("public") => Visibility::Public,
                Some("direct") => Visibility::Direct,
                _ => Visibility::Followers,
            };
            self.compose.protocol = ProtocolRoute::ActivityPub;
            self.active_mode = "home".to_string();
            self.active_screen = "compose".to_string();
            return Ok(
                "Notification reply context attached. Audience is still visible before sending."
                    .into(),
            );
        }
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
        let id =
            notification_id_from_row(row_id).ok_or_else(|| "no notification id".to_string())?;
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
        let (watch_type, target) = self.watch_request_from_row(row_id)?;
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
                    watch_type,
                    target,
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

    fn watch_request_from_row(&self, row_id: &str) -> Result<(String, String), String> {
        if let Some(post_url) = row_id.strip_prefix("url:") {
            if let Some(post) = self
                .data
                .search
                .public_posts
                .iter()
                .find(|post| post.url == post_url)
            {
                if let Some(target) = post.watch_target.as_deref() {
                    return Ok((
                        post.watch_type
                            .as_deref()
                            .unwrap_or_else(|| infer_watch_type(target))
                            .to_string(),
                        target.to_string(),
                    ));
                }
            }
            return Ok((infer_watch_type(post_url).to_string(), post_url.to_string()));
        }
        let target = target_from_row(row_id).ok_or_else(|| "no watch target".to_string())?;
        if let Some(actor) = self
            .data
            .search
            .public_actors
            .iter()
            .find(|actor| actor.follow_target.as_deref() == Some(target) || actor.id == target)
        {
            if let Some(watch_target) = actor.watch_target.as_deref() {
                return Ok((
                    actor
                        .watch_type
                        .as_deref()
                        .unwrap_or_else(|| infer_watch_type(watch_target))
                        .to_string(),
                    watch_target.to_string(),
                ));
            }
        }
        Ok((infer_watch_type(target).to_string(), target.to_string()))
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

    fn refresh_row(&mut self, row_id: &str) -> Result<String, String> {
        if row_id.starts_with("watch:") || row_id.starts_with("source:") {
            return self.refresh_source_or_watch(row_id);
        }
        self.refresh();
        Ok("Refreshed owner server state.".into())
    }

    fn retry_delivery(&self, row_id: &str) -> Result<String, String> {
        let id = delivery_id_from_row(row_id).ok_or_else(|| "no delivery id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview delivery retry queued.".into());
        }
        let client = self.client()?;
        let delivery: OwnerDelivery = self.runtime.block_on(async move {
            client
                .retry_delivery(id)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!(
            "Delivery {} requeued with {} retry attempt(s).",
            delivery.id,
            delivery.retry_count.unwrap_or_default()
        ))
    }

    fn cancel_delivery(&self, row_id: &str) -> Result<String, String> {
        let id = delivery_id_from_row(row_id).ok_or_else(|| "no delivery id".to_string())?;
        if self
            .settings
            .owner_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            return Ok("Preview delivery cancelled.".into());
        }
        let client = self.client()?;
        let delivery: OwnerDelivery = self.runtime.block_on(async move {
            client
                .cancel_delivery(id)
                .await
                .map_err(|error| error.to_string())
        })?;
        Ok(format!("Delivery {} cancelled.", delivery.id))
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
        self.block_actor_value(target, "Blocked from Dais Desk")
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
        let url = resolve_external_url(self, row_id)?;
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

    fn active_account_id(&self) -> String {
        active_account(&self.settings)
            .map(|account| account.id.clone())
            .unwrap_or_else(|| account_id_for(&self.settings.instance_url, &[]))
    }

    fn drafts_for_active_account(&self) -> Vec<StoredDraft> {
        let account_id = self.active_account_id();
        let mut drafts: Vec<StoredDraft> = self
            .drafts
            .drafts
            .iter()
            .filter(|draft| draft.account_id == account_id)
            .cloned()
            .collect();
        drafts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        drafts
    }

    fn save_current_draft_inner(&mut self) -> Result<String, String> {
        if self.compose.text.trim().is_empty()
            && self.compose.recipients.trim().is_empty()
            && self.compose.media_description.trim().is_empty()
            && self.compose.attachments.is_empty()
        {
            return Err("draft has no text, recipients, media description, or attachments".into());
        }
        let account_id = self.active_account_id();
        let updated_at = unix_timestamp_label();
        let id = draft_id_for(&account_id, &updated_at, &self.compose.text);
        let draft = StoredDraft {
            id: id.clone(),
            account_id,
            text: self.compose.text.trim().to_string(),
            visibility: self.compose.visibility.clone(),
            protocol: self.compose.protocol.clone(),
            encrypt: self.compose.encrypt,
            in_reply_to: self.compose.in_reply_to.clone(),
            audience_list_id: self.compose.audience_list_id.clone(),
            recipients: self.compose.recipients.trim().to_string(),
            media_description: self.compose.media_description.trim().to_string(),
            attachments: self.compose.attachments.clone(),
            updated_at,
        };
        self.drafts.drafts.retain(|existing| existing.id != id);
        self.drafts.drafts.push(draft);
        persist_drafts_to(&self.drafts_path, self.drafts.clone())?;
        Ok("Saved local draft for this account.".into())
    }

    fn open_draft(&mut self, draft_id: &str) -> Result<String, String> {
        let draft = self
            .drafts
            .drafts
            .iter()
            .find(|draft| draft.id == draft_id && draft.account_id == self.active_account_id())
            .cloned()
            .ok_or_else(|| "draft not found for the active account".to_string())?;
        self.compose = ComposeState {
            text: draft.text,
            visibility: draft.visibility,
            protocol: draft.protocol,
            encrypt: draft.encrypt,
            in_reply_to: draft.in_reply_to,
            audience_list_id: draft.audience_list_id,
            recipients: draft.recipients,
            media_description: draft.media_description,
            attachments: draft.attachments,
        };
        self.active_mode = "home".into();
        self.active_screen = "compose".into();
        Ok("Opened local draft in Compose.".into())
    }

    fn delete_draft(&mut self, draft_id: &str) -> Result<String, String> {
        let account_id = self.active_account_id();
        let before = self.drafts.drafts.len();
        self.drafts
            .drafts
            .retain(|draft| !(draft.id == draft_id && draft.account_id == account_id));
        if self.drafts.drafts.len() == before {
            return Err("draft not found for the active account".into());
        }
        persist_drafts_to(&self.drafts_path, self.drafts.clone())?;
        Ok("Deleted local draft.".into())
    }

    fn use_audience_in_compose(&mut self, list_id: &str) -> Result<String, String> {
        let list = self
            .data
            .snapshot
            .audience_lists
            .iter()
            .find(|list| list.id == list_id)
            .ok_or_else(|| "audience group not found".to_string())?;
        self.compose.visibility = Visibility::Direct;
        self.compose.protocol = ProtocolRoute::ActivityPub;
        self.compose.audience_list_id = Some(list.id.clone());
        self.active_mode = "home".into();
        self.active_screen = "compose".into();
        Ok(format!(
            "Compose is targeting {} ({} member(s)). Review the visibility summary before sending.",
            list.name, list.member_count
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

    fn rows_for_active_screen_for_projection(&self) -> Vec<UiRow> {
        match self.active_screen.as_str() {
            "today" => self.home_today_rows(),
            "reading" => self.reading_rows(),
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

    fn rows_for_active_screen(&self) -> Vec<UiRow> {
        let mut rows = self.rows_for_active_screen_for_projection();
        let suppress_secondary = matches!(
            self.active_screen.as_str(),
            "today"
                | "inbox"
                | "find"
                | "relationship"
                | "friends"
                | "followers"
                | "following"
                | "watches"
                | "audience"
                | "blocks"
        );
        if suppress_secondary {
            rows.iter_mut().for_each(|row| row.secondary = s(""));
        }
        rows
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
                ("reading", "Reading", self.reading_rows().len()),
                ("inbox", "Inbox", self.inbox_rows().len()),
                ("compose", "Compose", 0),
                ("posts", "My Posts", self.data.snapshot.posts.len()),
                ("saved", "Saved & Drafts", self.saved_rows().len()),
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
            "reading" => "Reading".into(),
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
            "reading" => {
                "Posts from followed accounts, private watches, and reading sources.".into()
            }
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

    fn reading_rows(&self) -> Vec<UiRow> {
        let mut rows: Vec<UiRow> = self
            .data
            .snapshot
            .home_timeline
            .iter()
            .map(reading_timeline_row)
            .collect();
        rows.extend(
            self.data
                .watches
                .items
                .iter()
                .map(|item| reading_source_item_row(item, "Watched public post", "Watch")),
        );
        rows.extend(
            self.data
                .sources
                .items
                .iter()
                .map(|item| reading_source_item_row(item, "Source post", "Source")),
        );
        if rows.is_empty() {
            rows.push(empty_state_row(
                "reading:empty",
                "No reading stream yet",
                "Follow an account, add a private Watch, or add an RSS/Atom source to populate this stream.",
                "Find people",
            ));
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
        let indicator = compose_audience_indicator(&self.compose);
        let mut rows = vec![row(
            "compose:privacy",
            "Audience preview",
            "Private by default",
            &compose_warning(&self.compose),
            &indicator.label,
            indicator.tone,
            "Save draft",
            "",
        )];
        rows.push(row(
            "compose:visibility-summary",
            "Who can see this",
            "Review before sending",
            &compose_audience_summary(&self.compose, &self.data.snapshot),
            &indicator.label,
            indicator.tone,
            "",
            "",
        ));
        if let Some(reply) = &self.compose.in_reply_to {
            rows.push(row(
                &format!("post:{reply}"),
                "Reply context",
                "This reply will use the audience above",
                &reply_context_summary(reply, &self.data),
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
        let mut rows = vec![row(
            "saved:owner-only",
            "Saved posts",
            "Owner-only bookmarks",
            "Server-backed saved posts are not implemented yet; local drafts below stay on this device.",
            "Owner-only",
            "ok",
            "",
            "",
        )];
        rows.extend(
            self.drafts_for_active_account()
                .into_iter()
                .map(|draft| draft_row(&draft)),
        );
        if rows.len() == 1 {
            rows.push(row(
                "drafts:empty",
                "No saved drafts",
                "Compose can save unsent work locally",
                "Drafts preserve text, audience, route, recipients, reply context, and attached media URLs.",
                "Local",
                "muted",
                "",
                "",
            ));
        }
        rows
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
        let mut rows = Vec::new();
        rows.extend(self.data.snapshot.friends.iter().map(friend_row));
        rows.extend(self.data.snapshot.followers.iter().map(follower_row));
        rows.extend(self.data.snapshot.following.iter().map(following_row));
        if rows.is_empty() {
            rows.push(empty_state_row(
                "relationship:empty",
                "No relationships yet",
                "Find people to follow, approve follower requests, or Watch public accounts privately.",
                "Find people",
            ));
        }
        rows
    }

    fn friend_rows(&self) -> Vec<UiRow> {
        let rows: Vec<UiRow> = self.data.snapshot.friends.iter().map(friend_row).collect();
        if rows.is_empty() {
            vec![empty_state_row(
                "friends:empty",
                "No friends yet",
                "Friends appear after you approve someone as a follower and follow them back. Friend relationships are owner-only.",
                "Find people",
            )]
        } else {
            rows
        }
    }

    fn follower_rows(&self) -> Vec<UiRow> {
        let rows: Vec<UiRow> = self
            .data
            .snapshot
            .followers
            .iter()
            .map(follower_row)
            .collect();
        if rows.is_empty() {
            vec![empty_state_row(
                "followers:empty",
                "No followers yet",
                "Follow requests appear here for approval before anyone can read follower-only posts.",
                "",
            )]
        } else {
            rows
        }
    }

    fn following_rows(&self) -> Vec<UiRow> {
        let rows: Vec<UiRow> = self
            .data
            .snapshot
            .following
            .iter()
            .map(following_row)
            .collect();
        if rows.is_empty() {
            vec![empty_state_row(
                "following:empty",
                "You are not following anyone yet",
                "Use Find to follow an account. Use Watch when you only want to read public posts without sending a follow request.",
                "Find people",
            )]
        } else {
            rows
        }
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
        if rows.is_empty() {
            rows.push(empty_state_row(
                "watches:empty",
                "No watches or sources yet",
                "Watch a public account or add an RSS/Atom source to read public posts without creating a remote relationship.",
                "Add Watch",
            ));
        }
        rows
    }

    fn audience_rows(&self) -> Vec<UiRow> {
        let rows: Vec<UiRow> = self
            .data
            .snapshot
            .audience_lists
            .iter()
            .map(audience_row)
            .collect();
        if rows.is_empty() {
            vec![empty_state_row(
                "audience:empty",
                "No audience groups yet",
                "Create a group for small, intentional sharing sets such as close friends or project collaborators.",
                "",
            )]
        } else {
            rows
        }
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
        if rows.is_empty() {
            rows.push(empty_state_row(
                "blocks:empty",
                "No blocks or allowed hosts",
                "Blocks, mutes, and closed-network allowlist entries appear here when configured.",
                "",
            ));
        }
        rows
    }

    fn health_rows(&self) -> Vec<UiRow> {
        let stats = &self.data.stats;
        let moderation = &self.data.snapshot.moderation;
        let settings = &self.data.snapshot.settings;
        let owner_api_ok = self.data.api_error.is_none() && settings.owner_token_present;
        let privacy_ok = matches!(
            settings.default_visibility,
            Visibility::Followers | Visibility::Direct
        ) && moderation.require_authorized_fetch
            && moderation.manually_approves_followers;
        let failed = stats.deliveries_failed;
        let queued = stats.deliveries_queued + stats.deliveries_retry;
        let review = stats.notifications_unread + moderation.reply_queue_count;
        let mut rows = vec![
            row(
                "health:owner-api",
                "Owner API",
                if owner_api_ok {
                    "Authenticated"
                } else {
                    "Needs token"
                },
                self.data
                    .api_error
                    .as_deref()
                    .unwrap_or("Owner API token is present and the latest snapshot loaded."),
                if owner_api_ok { "OK" } else { "Review" },
                if owner_api_ok { "ok" } else { "warn" },
                "Refresh",
                "Copy evidence",
            ),
            row(
                "health:privacy",
                "Privacy posture",
                &format!(
                    "{} via {}",
                    visibility_label(&settings.default_visibility),
                    protocol_label(&settings.default_protocol)
                ),
                &format!(
                    "Authorized fetch: {}. Manual follower approval: {}. Closed network: {}.",
                    on_off(moderation.require_authorized_fetch),
                    on_off(moderation.manually_approves_followers),
                    on_off(moderation.closed_network)
                ),
                if privacy_ok { "Private" } else { "Review" },
                if privacy_ok { "ok" } else { "warn" },
                "",
                "Copy evidence",
            ),
            row(
                "health:queues",
                "Attention queues",
                &format!("{review} social review, {failed} failed delivery"),
                &format!(
                    "{} unread notifications, {} moderation replies, {} queued/retrying deliveries.",
                    stats.notifications_unread, moderation.reply_queue_count, queued
                ),
                if failed > 0 || review > 0 {
                    "Attention"
                } else {
                    "Clear"
                },
                if failed > 0 || review > 0 {
                    "warn"
                } else {
                    "ok"
                },
                if failed > 0 { "Inspect delivery" } else { "" },
                "Copy evidence",
            ),
            row(
                "health:graph",
                "Social graph",
                "Owner-visible relationship counts",
                &format!(
                    "{} approved followers, {} pending, {} following, {} friends.",
                    stats.followers_approved,
                    stats.followers_pending,
                    stats.following_total,
                    self.data.snapshot.friends.len()
                ),
                "Private",
                "ok",
                "",
                "Copy evidence",
            ),
        ];
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
            "Copy evidence",
        ));
        rows.extend(self.data.snapshot.diagnostics.iter().map(diagnostic_row));
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
                "",
                "",
            ),
        ]
    }

    fn account_rows_as_ui(&self) -> Vec<UiRow> {
        let can_delete = self.settings.accounts.len() > 1;
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
                    if account.active && account.owner_token_present {
                        "Validate token"
                    } else if account.active {
                        ""
                    } else {
                        "Switch"
                    },
                    if can_delete { "Delete" } else { "" },
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
            rows.push(selected.clone());
            rows.extend(selected_visibility_inspector_rows(&selected));
        }
        rows.extend(self.actor_profile_inspector_rows(selected_row));
        rows.extend(self.external_link_inspector_rows(selected_row));
        rows.extend(self.notification_inspector_rows(selected_row));
        rows.extend(self.post_detail_inspector_rows(selected_row));
        rows.extend(self.delivery_inspector_rows(selected_row));
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
            "",
            "",
        ));
        rows
    }

    fn delivery_inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let Some(delivery_id) = selected_row.strip_prefix("delivery:") else {
            return Vec::new();
        };
        let Some(delivery) = self
            .data
            .deliveries
            .iter()
            .find(|delivery| delivery.id == delivery_id)
        else {
            return Vec::new();
        };
        let (primary, secondary) = delivery_action_pair(delivery.status.as_str());
        let mut rows = vec![row(
            &format!("delivery-detail:{}", delivery.id),
            "Delivery detail",
            &format!(
                "{} {}",
                delivery.protocol,
                delivery.activity_type.as_deref().unwrap_or("activity")
            ),
            &format!(
                "Status: {}. Retry count: {}. Last attempt: {}. Created: {}.",
                delivery.status,
                delivery.retry_count.unwrap_or_default(),
                delivery.last_attempt_at.as_deref().unwrap_or("never"),
                delivery.created_at.as_deref().unwrap_or("unknown")
            ),
            &delivery.status,
            delivery_tone(delivery.status.as_str()),
            primary,
            secondary,
        )];
        rows.push(row(
            &format!("url:{}", delivery.target_url),
            "Remote target",
            delivery.target_type.as_deref().unwrap_or("recipient"),
            &delivery.target_url,
            "Target",
            "info",
            if delivery.target_url.starts_with("http://")
                || delivery.target_url.starts_with("https://")
            {
                "Open link"
            } else {
                ""
            },
            "Copy evidence",
        ));
        if let Some(error) = delivery
            .error_message
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            rows.push(row(
                &format!("delivery-failure:{}", delivery.id),
                "Failure explanation",
                "Operator action",
                &delivery_failure_explanation(error),
                "Failure",
                "warn",
                if matches!(delivery.status.as_str(), "failed" | "retry") {
                    "Retry delivery"
                } else {
                    ""
                },
                if delivery.status != "delivered" {
                    "Cancel delivery"
                } else {
                    ""
                },
            ));
        }
        rows
    }

    fn external_link_inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let Some(url) = self.external_url_for_selected_content(selected_row) else {
            return Vec::new();
        };
        vec![row(
            &format!("url:{url}"),
            "External link",
            &compact_url(&url),
            "Open the linked web page in your default browser. The full URL is hidden from the main reading row to keep the timeline readable.",
            "Link",
            "info",
            "Open link",
            "",
        )]
    }

    fn actor_profile_inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let Some((actor_id, label, relationship)) = self.actor_context_for_row(selected_row) else {
            return Vec::new();
        };
        if !actor_id.starts_with("http://") && !actor_id.starts_with("https://") {
            return Vec::new();
        }
        vec![row(
            &format!("url:{actor_id}"),
            label,
            &relationship,
            "Open this actor profile, or Watch it to read public posts privately without sending a follow request.",
            "Profile",
            "info",
            "Open original",
            "Watch",
        )]
    }

    fn actor_context_for_row(&self, selected_row: &str) -> Option<(String, &'static str, String)> {
        if let Some(object_id) = object_id_from_row(selected_row) {
            return self
                .data
                .snapshot
                .home_timeline
                .iter()
                .find(|post| post.object_id == object_id)
                .map(|post| {
                    (
                        post.actor_id.clone(),
                        "Author profile",
                        relationship_for_actor(&self.data.snapshot, &post.actor_id),
                    )
                });
        }
        if let Some(actor) = selected_row.strip_prefix("actor:") {
            return Some((
                actor.to_string(),
                "Friend profile",
                "Mutual private sharing".to_string(),
            ));
        }
        if let Some(actor) = selected_row.strip_prefix("follower:") {
            return Some((
                actor.to_string(),
                "Follower profile",
                "They follow you; approval controls follower-only access.".to_string(),
            ));
        }
        if let Some(actor) = selected_row.strip_prefix("following:") {
            return Some((
                actor.to_string(),
                "Following profile",
                "You follow this account; the remote server may know.".to_string(),
            ));
        }
        None
    }

    fn external_url_for_selected_content(&self, selected_row: &str) -> Option<String> {
        let from_text =
            |parts: Vec<Option<&str>>| parts.into_iter().flatten().find_map(extract_first_url);
        if let Some(object_id) = object_id_from_row(selected_row) {
            return self
                .data
                .snapshot
                .home_timeline
                .iter()
                .find(|post| post.object_id == object_id)
                .and_then(|post| from_text(vec![post.content_html.as_deref(), Some(&post.content)]))
                .or_else(|| {
                    self.data
                        .snapshot
                        .posts
                        .iter()
                        .find(|post| post.id == object_id)
                        .and_then(|post| from_text(vec![Some(&post.content)]))
                })
                .or_else(|| {
                    self.data
                        .post_detail
                        .as_ref()
                        .filter(|detail| detail.post.id == object_id)
                        .and_then(|detail| {
                            from_text(vec![
                                detail.content_html.as_deref(),
                                Some(&detail.post.content),
                            ])
                        })
                });
        }
        if let Some(notification_id) = notification_id_from_row(selected_row) {
            return self
                .data
                .notifications
                .iter()
                .find(|notice| notice.id == notification_id)
                .and_then(|notice| {
                    from_text(vec![
                        notice.context_post_content_html.as_deref(),
                        notice.context_post_content.as_deref(),
                        notice.content.as_deref(),
                    ])
                });
        }
        None
    }

    fn notification_inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let Some(notification_id) = selected_row.strip_prefix("notification:") else {
            return Vec::new();
        };
        let Some(notice) = self
            .data
            .notifications
            .iter()
            .find(|notice| notice.id == notification_id)
        else {
            return Vec::new();
        };
        let actor = notice
            .actor_display_name
            .as_deref()
            .or(notice.actor_username.as_deref())
            .unwrap_or(&notice.actor_id);
        let mut rows = vec![row_with_kind(
            "notification",
            &format!("notification-detail:{}", notice.id),
            "What happened",
            actor,
            &format!(
                "{}. {}",
                notification_action_sentence(notice.kind.as_str()),
                if json_truthy(&notice.read) {
                    "This notification is already marked read."
                } else {
                    "This notification is unread."
                }
            ),
            notice.created_at.as_deref().unwrap_or("notification"),
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
                ""
            } else {
                "Mark read"
            },
            "",
        )];
        if matches!(notice.kind.as_str(), "reply" | "mention") {
            if let Some(reply) = notice.content.as_deref() {
                rows.push(row_with_kind(
                    "post",
                    &format!("notification-reply:{}", notice.id),
                    if notice.kind == "mention" {
                        "Mention text"
                    } else {
                        "Reply text"
                    },
                    actor,
                    &preview_markdown_safe(reply),
                    "This is the new content from the other account.",
                    "Reply",
                    "info",
                    if notice.context_post_id.is_some() || notice.post_id.is_some() {
                        "Reply"
                    } else {
                        ""
                    },
                    "",
                ));
            }
        }
        let context_source = notice
            .context_post_content_html
            .as_deref()
            .or(notice.context_post_content.as_deref())
            .or_else(|| {
                (!matches!(notice.kind.as_str(), "reply" | "mention"))
                    .then_some(notice.content.as_deref())
                    .flatten()
            });
        if let Some(context) = context_source {
            rows.push(row_with_kind(
                "post",
                &format!("notification-context:{}", notice.id),
                if matches!(notice.kind.as_str(), "reply" | "mention") {
                    "Original post"
                } else {
                    "Original context"
                },
                notice
                    .context_post_published_at
                    .as_deref()
                    .unwrap_or("Related post"),
                &preview_markdown_safe(context),
                notice
                    .context_post_visibility
                    .as_deref()
                    .map(visibility_explanation_str)
                    .unwrap_or("The server did not include original-post visibility."),
                notice
                    .context_post_visibility
                    .as_deref()
                    .map(visibility_string_label)
                    .unwrap_or("Context"),
                notice
                    .context_post_visibility
                    .as_deref()
                    .map(visibility_tone)
                    .unwrap_or("info"),
                if notice.context_post_id.is_some() || notice.post_id.is_some() {
                    "Open context"
                } else {
                    ""
                },
                if matches!(notice.kind.as_str(), "reply" | "mention") {
                    "Reply"
                } else {
                    ""
                },
            ));
        }
        rows
    }

    fn post_detail_inspector_rows(&self, selected_row: &str) -> Vec<UiRow> {
        let Some(detail) = &self.data.post_detail else {
            return Vec::new();
        };
        let Some(selected_object_id) = object_id_from_row(selected_row) else {
            return Vec::new();
        };
        if detail.post.id != selected_object_id {
            return Vec::new();
        }
        let mut rows = vec![row(
            &format!("post-detail:{}", detail.post.id),
            "Thread detail",
            detail
                .post
                .published_at
                .as_deref()
                .unwrap_or("Selected post"),
            &format!(
                "{} replies, {} likes, {} boosts. {}",
                detail.replies.len(),
                detail.likes.len(),
                detail.boosts.len(),
                detail
                    .in_reply_to
                    .as_deref()
                    .map(|reply| format!("In reply to {reply}."))
                    .unwrap_or_default()
            ),
            "Thread",
            "info",
            "Reply",
            "Delete",
        )];
        for (index, reply) in detail.replies.iter().take(6).enumerate() {
            rows.push(reply_activity_row(&detail.post.id, index, reply));
        }
        if detail.replies.len() > 6 {
            rows.push(row(
                &format!("post-detail:{}:more-replies", detail.post.id),
                "More replies",
                "Thread continues",
                &format!(
                    "{} additional reply/replies are available from server detail.",
                    detail.replies.len() - 6
                ),
                "Thread",
                "info",
                "",
                "",
            ));
        }
        if !detail.likes.is_empty() || !detail.boosts.is_empty() {
            rows.push(row(
                &format!("post-detail:{}:activity", detail.post.id),
                "Post activity",
                "Likes and boosts",
                &format!(
                    "{} like(s), {} boost(s). These are social signals, not replies.",
                    detail.likes.len(),
                    detail.boosts.len()
                ),
                "Activity",
                "info",
                "",
                "",
            ));
        }
        for attachment in &detail.post.attachments {
            if let Some(url) = attachment_url(attachment) {
                rows.push(row(
                    &format!("media:{url}"),
                    "Media attachment",
                    attachment_media_type(attachment)
                        .as_deref()
                        .unwrap_or("Attachment"),
                    &url,
                    "Media",
                    "info",
                    "Open link",
                    "Revoke media",
                ));
            }
        }
        rows
    }

    fn find_row(&self, row_id: &str) -> Option<UiRow> {
        self.rows_for_active_screen_for_projection()
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

    fn context_row_for(&self, row_id: &str) -> Option<String> {
        if row_id.starts_with("post:") || row_id.starts_with("timeline:") {
            return Some(row_id.to_string());
        }
        if let Some(id) = notification_id_from_row(row_id) {
            return self
                .data
                .notifications
                .iter()
                .find(|notice| notice.id == id)
                .and_then(|notice| {
                    notice
                        .context_post_id
                        .as_deref()
                        .or(notice.post_id.as_deref())
                })
                .map(|post_id| format!("post:{post_id}"));
        }
        if let Some(id) = row_id.strip_prefix("delivery:") {
            return self
                .data
                .deliveries
                .iter()
                .find(|delivery| delivery.id == id)
                .map(|delivery| format!("post:{}", delivery.post_id));
        }
        None
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

trait IfEmptyStr {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmptyStr for &str {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self.to_string()
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
    window.on_run_filtered_search(
        move |query,
              scope,
              provider,
              result_type,
              servers,
              sort,
              since,
              until,
              author,
              mentions,
              lang,
              domain,
              url,
              tags,
              confirm| {
            if let Some(window) = weak.upgrade() {
                ctrl.borrow_mut().run_filtered_search(SearchFormInput {
                    query: query.as_str(),
                    scope: scope.as_str(),
                    provider: provider.as_str(),
                    result_type: result_type.as_str(),
                    servers: servers.as_str(),
                    sort: sort.as_str(),
                    since: since.as_str(),
                    until: until.as_str(),
                    author: author.as_str(),
                    mentions: mentions.as_str(),
                    lang: lang.as_str(),
                    domain: domain.as_str(),
                    url: url.as_str(),
                    tags: tags.as_str(),
                    confirm_public_sensitive: confirm,
                });
                apply_controller_projection(&window, &ctrl);
            }
        },
    );

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_add_source(move |source_type, url, title, cadence| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().add_source_from_form(
                source_type.as_str(),
                url.as_str(),
                title.as_str(),
                cadence.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_add_watch(move |watch_type, target, title, cadence| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().add_watch_from_form(
                watch_type.as_str(),
                target.as_str(),
                title.as_str(),
                cadence.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_preview_profile(move |actor_type, display_name, summary, icon, image| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().preview_profile_from_form(
                actor_type.as_str(),
                display_name.as_str(),
                summary.as_str(),
                icon.as_str(),
                image.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_save_profile(move |actor_type, display_name, summary, icon, image| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().save_profile_from_form(
                actor_type.as_str(),
                display_name.as_str(),
                summary.as_str(),
                icon.as_str(),
                image.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_save_audience(move |id, name, description, categories, members| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().save_audience_from_form(
                id.as_str(),
                name.as_str(),
                description.as_str(),
                categories.as_str(),
                members.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_delete_audience(move |id| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().delete_audience_from_form(id.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_save_moderation(move |reply_policy, ai_enabled, ai_model, ai_budget| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().save_moderation_from_form(
                reply_policy.as_str(),
                ai_enabled,
                ai_model.as_str(),
                ai_budget.as_str(),
            );
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_save_settings(
        move |default_visibility,
              default_protocol,
              require_authorized_fetch,
              manually_approves_followers,
              closed_network| {
            if let Some(window) = weak.upgrade() {
                ctrl.borrow_mut().save_settings_from_form(
                    default_visibility.as_str(),
                    default_protocol.as_str(),
                    require_authorized_fetch,
                    manually_approves_followers,
                    closed_network,
                );
                apply_controller_projection(&window, &ctrl);
            }
        },
    );

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_block_actor(move |actor_id, reason| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut()
                .block_actor_from_form(actor_id.as_str(), reason.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_block_domain(move |domain, reason| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut()
                .block_domain_from_form(domain.as_str(), reason.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_allow_host(move |host, note| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut()
                .allow_host_from_form(host.as_str(), note.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_disallow_host(move |host| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().disallow_host_from_form(host.as_str());
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_choose_media_file(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().choose_media_file();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_upload_media(
        move |file_path, media_type, description, access, expires_seconds, authorized_fetch| {
            if let Some(window) = weak.upgrade() {
                ctrl.borrow_mut().upload_media_from_form(
                    file_path.as_str(),
                    media_type.as_str(),
                    description.as_str(),
                    access.as_str(),
                    expires_seconds.as_str(),
                    authorized_fetch,
                );
                apply_controller_projection(&window, &ctrl);
            }
        },
    );

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_revoke_media(move |url| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().revoke_media_from_form(url.as_str());
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
                    window.get_compose_audience_list().as_str(),
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
                    window.get_compose_audience_list().as_str(),
                    window.get_compose_media_description().as_str(),
                    window.get_compose_encrypt(),
                );
                controller.compose_set_protocol(value.as_str());
            }
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_compose_send(move || {
        if let Some(window) = weak.upgrade() {
            {
                let mut controller = ctrl.borrow_mut();
                controller.update_compose_from_ui(
                    window.get_compose_text().as_str(),
                    window.get_compose_recipients().as_str(),
                    window.get_compose_audience_list().as_str(),
                    window.get_compose_media_description().as_str(),
                    window.get_compose_encrypt(),
                );
                controller.compose_send();
            }
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_move_selected_row_up(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().move_row_selection_previous();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_move_selected_row_down(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().move_row_selection_next();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_move_selected_row_home(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().select_first_row();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_move_selected_row_end(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().select_last_row();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_activate_selected_row(move || {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().execute_selected_row_default_action();
            apply_controller_projection(&window, &ctrl);
        }
    });

    let weak = window.as_weak();
    let ctrl = controller.clone();
    window.on_select_row_by_prefix(move |prefix| {
        if let Some(window) = weak.upgrade() {
            ctrl.borrow_mut().set_row_match_from_prefix(prefix.as_str());
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
    window.set_compose_audience_list(s(&projection.compose_audience_list));
    window.set_compose_media_description(s(&projection.compose_media_description));
    window.set_compose_encrypt(projection.compose_encrypt);
    window.set_compose_visibility(s(&projection.compose_visibility));
    window.set_compose_protocol(s(&projection.compose_protocol));
    window.set_compose_warning(s(&projection.compose_warning));
    window.set_compose_audience_summary(s(&projection.compose_audience_summary));
    window.set_compose_can_send(projection.compose_can_send);
    window.set_account_label(s(&projection.account_label));
    window.set_account_url(s(&projection.account_url));
    window.set_account_token(s(&projection.account_token));
    window.set_search_scope(s(&projection.search_scope));
    window.set_search_provider(s(&projection.search_provider));
    window.set_search_type(s(&projection.search_type));
    window.set_search_servers(s(&projection.search_servers));
    window.set_search_sort(s(&projection.search_sort));
    window.set_search_since(s(&projection.search_since));
    window.set_search_until(s(&projection.search_until));
    window.set_search_author(s(&projection.search_author));
    window.set_search_mentions(s(&projection.search_mentions));
    window.set_search_lang(s(&projection.search_lang));
    window.set_search_domain(s(&projection.search_domain));
    window.set_search_url(s(&projection.search_url));
    window.set_search_tags(s(&projection.search_tags));
    window.set_search_confirm_public_sensitive(projection.search_confirm_public_sensitive);
    window.set_source_type(s(&projection.source_type));
    window.set_source_url(s(&projection.source_url));
    window.set_source_title(s(&projection.source_title));
    window.set_source_cadence(s(&projection.source_cadence));
    window.set_watch_type(s(&projection.watch_type));
    window.set_watch_target(s(&projection.watch_target));
    window.set_watch_title(s(&projection.watch_title));
    window.set_watch_cadence(s(&projection.watch_cadence));
    window.set_profile_actor_type(s(&projection.profile_actor_type));
    window.set_profile_display_name(s(&projection.profile_display_name));
    window.set_profile_summary(s(&projection.profile_summary));
    window.set_profile_icon(s(&projection.profile_icon));
    window.set_profile_image(s(&projection.profile_image));
    window.set_profile_preview(s(&projection.profile_preview));
    window.set_audience_id(s(&projection.audience_id));
    window.set_audience_name(s(&projection.audience_name));
    window.set_audience_description(s(&projection.audience_description));
    window.set_audience_categories(s(&projection.audience_categories));
    window.set_audience_members(s(&projection.audience_members));
    window.set_moderation_reply_policy(s(&projection.moderation_reply_policy));
    window.set_moderation_ai_enabled(projection.moderation_ai_enabled);
    window.set_moderation_ai_model(s(&projection.moderation_ai_model));
    window.set_moderation_ai_budget(s(&projection.moderation_ai_budget));
    window.set_moderation_block_actor(s(&projection.moderation_block_actor));
    window.set_moderation_block_domain(s(&projection.moderation_block_domain));
    window.set_moderation_block_reason(s(&projection.moderation_block_reason));
    window.set_moderation_allow_host(s(&projection.moderation_allow_host));
    window.set_moderation_allow_note(s(&projection.moderation_allow_note));
    window.set_settings_default_visibility(s(&projection.settings_default_visibility));
    window.set_settings_default_protocol(s(&projection.settings_default_protocol));
    window.set_settings_require_authorized_fetch(projection.settings_require_authorized_fetch);
    window
        .set_settings_manually_approves_followers(projection.settings_manually_approves_followers);
    window.set_settings_closed_network(projection.settings_closed_network);
    window.set_media_file_path(s(&projection.media_file_path));
    window.set_media_type(s(&projection.media_type));
    window.set_media_description(s(&projection.media_description));
    window.set_media_access(s(&projection.media_access));
    window.set_media_expires_seconds(s(&projection.media_expires_seconds));
    window.set_media_authorized_fetch(projection.media_authorized_fetch);
    window.set_media_revoke_url(s(&projection.media_revoke_url));
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
    row_with_kind(
        "generic", id, title, subtitle, detail, "", chip, tone, primary, secondary,
    )
}

fn row_with_kind(
    kind: &str,
    id: &str,
    title: &str,
    subtitle: &str,
    detail: &str,
    meta: &str,
    chip: &str,
    tone: &str,
    primary: &str,
    secondary: &str,
) -> UiRow {
    UiRow {
        id: s(id),
        kind: s(kind),
        title: s(&clean_text(title)),
        subtitle: s(&clean_text(subtitle)),
        detail: s(&clean_text(detail)),
        meta: s(&clean_text(meta)),
        chip: s(chip),
        tone: s(tone),
        primary: s(primary),
        secondary: s(secondary),
    }
}

fn empty_state_row(id: &str, title: &str, detail: &str, primary: &str) -> UiRow {
    row_with_kind(
        "empty",
        id,
        title,
        "Nothing needs attention here",
        detail,
        "Next step",
        "Empty",
        "info",
        primary,
        "",
    )
}

fn account_row(account: OwnerAccountSummary, can_delete: bool) -> AccountRow {
    AccountRow {
        id: s(&account.id),
        title: s(&account.label),
        subtitle: s(&account.instance_url),
        active: account.active,
        token: account.owner_token_present,
        can_delete,
    }
}

#[derive(Clone, Debug)]
struct AudienceIndicator {
    label: String,
    tone: &'static str,
    explanation: String,
}

fn audience_indicator_for_visibility(visibility: &Visibility) -> AudienceIndicator {
    match visibility {
        Visibility::Public => AudienceIndicator {
            label: "Public web".into(),
            tone: "warn",
            explanation: "Anyone who can read public web or federated public routes may be able to read this.".into(),
        },
        Visibility::Unlisted => AudienceIndicator {
            label: "Unlisted".into(),
            tone: "info",
            explanation: "Readable by link or addressed/federated audiences, but not promoted as a public timeline item.".into(),
        },
        Visibility::Followers => AudienceIndicator {
            label: "Followers".into(),
            tone: "ok",
            explanation: "Intended for approved followers or friends; follower servers may receive delivered copies.".into(),
        },
        Visibility::Direct => AudienceIndicator {
            label: "Direct".into(),
            tone: "ok",
            explanation: "Intended only for named recipients or a selected audience group.".into(),
        },
    }
}

fn audience_indicator_for_string(visibility: &str) -> AudienceIndicator {
    match visibility.trim().to_ascii_lowercase().as_str() {
        "public" => audience_indicator_for_visibility(&Visibility::Public),
        "unlisted" => audience_indicator_for_visibility(&Visibility::Unlisted),
        "followers" | "private" => audience_indicator_for_visibility(&Visibility::Followers),
        "direct" => audience_indicator_for_visibility(&Visibility::Direct),
        _ => AudienceIndicator {
            label: "Unknown".into(),
            tone: "info",
            explanation: "The server did not include precise audience information.".into(),
        },
    }
}

fn compose_audience_indicator(compose: &ComposeState) -> AudienceIndicator {
    audience_indicator_for_target(
        &compose.visibility,
        compose.encrypt,
        split_list(&compose.recipients).len(),
        compose
            .audience_list_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty()),
    )
}

fn draft_audience_indicator(draft: &StoredDraft) -> AudienceIndicator {
    audience_indicator_for_target(
        &draft.visibility,
        draft.encrypt,
        split_list(&draft.recipients).len(),
        draft
            .audience_list_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty()),
    )
}

fn audience_indicator_for_target(
    visibility: &Visibility,
    encrypted: bool,
    recipient_count: usize,
    has_group: bool,
) -> AudienceIndicator {
    if encrypted && matches!(visibility, Visibility::Direct) {
        return if recipient_count == 1 && !has_group {
            AudienceIndicator {
                label: "E2EE 1:1".into(),
                tone: "ok",
                explanation: "Encrypted direct post for one named recipient.".into(),
            }
        } else {
            AudienceIndicator {
                label: "E2EE group".into(),
                tone: "ok",
                explanation: if has_group {
                    "Encrypted direct post for the selected audience group.".into()
                } else {
                    format!("Encrypted direct post for {recipient_count} named recipients.")
                },
            }
        };
    }
    if matches!(visibility, Visibility::Direct) {
        if has_group {
            return AudienceIndicator {
                label: "Group".into(),
                tone: "ok",
                explanation: "Direct post for a selected audience group.".into(),
            };
        }
        if recipient_count == 1 {
            return AudienceIndicator {
                label: "1 person".into(),
                tone: "ok",
                explanation: "Direct post for one named recipient.".into(),
            };
        }
        if recipient_count > 1 {
            return AudienceIndicator {
                label: format!("{recipient_count} people"),
                tone: "ok",
                explanation: format!(
                    "Direct post for {recipient_count} individually named recipients."
                ),
            };
        }
    }
    audience_indicator_for_visibility(visibility)
}

fn social_post_meta(
    visibility: &str,
    protocol: Option<&str>,
    published_at: Option<&str>,
    in_reply_to: Option<&str>,
    replies: u64,
    likes: u64,
    boosts: u64,
) -> String {
    let mut parts = vec![visibility_string_label(visibility).to_string()];
    if let Some(protocol) = protocol.filter(|value| !value.trim().is_empty()) {
        parts.push(protocol.to_string());
    }
    if in_reply_to.is_some() {
        parts.push("reply".into());
    }
    if let Some(published_at) = published_at.filter(|value| !value.trim().is_empty()) {
        parts.push(published_at.to_string());
    }
    let activity = [(replies, "reply"), (likes, "like"), (boosts, "boost")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, label)| {
            let plural = if label == "reply" { "replies" } else { "s" };
            if count == 1 {
                format!("1 {label}")
            } else if label == "reply" {
                format!("{count} {plural}")
            } else {
                format!("{count} {label}{plural}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !activity.is_empty() {
        parts.push(activity);
    }
    parts.join(" · ")
}

fn relationship_for_actor(snapshot: &OwnerSnapshotBundle, actor_id: &str) -> String {
    if snapshot
        .friends
        .iter()
        .any(|friend| friend.friend_actor_id == actor_id)
    {
        "Friend: mutual private sharing relationship.".into()
    } else if snapshot
        .following
        .iter()
        .any(|following| following.target_actor_id == actor_id)
    {
        "Following: you may have sent a remote relationship signal.".into()
    } else if snapshot
        .followers
        .iter()
        .any(|follower| follower.follower_actor_id == actor_id)
    {
        "Follower: approval controls follower-only access.".into()
    } else {
        "Public/discovered actor; use Watch for private public-post monitoring.".into()
    }
}

fn timeline_row(post: &OwnerTimelinePost) -> UiRow {
    let author = post
        .actor_display_name
        .as_deref()
        .or(post.actor_username.as_deref())
        .unwrap_or(&post.actor_id);
    let indicator = audience_indicator_for_string(&post.visibility);
    let meta = social_post_meta(
        &post.visibility,
        post.protocol.as_deref(),
        post.published_at.as_deref(),
        post.in_reply_to.as_deref(),
        post.reply_count,
        post.like_count,
        post.boost_count,
    );
    row_with_kind(
        "post",
        &format!("timeline:{}", post.object_id),
        author,
        post.actor_username.as_deref().unwrap_or(&post.actor_id),
        &preview_markdown_safe(post.content_html.as_deref().unwrap_or(&post.content)),
        &meta,
        &indicator.label,
        indicator.tone,
        "Reply",
        "Favorite",
    )
}

fn reading_timeline_row(post: &OwnerTimelinePost) -> UiRow {
    let mut row = timeline_row(post);
    row.subtitle = s(&format!("Following · {}", row.subtitle));
    row
}

fn post_row(post: &OwnerPost) -> UiRow {
    let title = post.title.as_deref().unwrap_or("My post");
    let indicator = if post.encrypted {
        AudienceIndicator {
            label: "E2EE".into(),
            tone: "ok",
            explanation:
                "Encrypted post. Exact recipient count is not included in this post summary.".into(),
        }
    } else {
        audience_indicator_for_visibility(&post.visibility)
    };
    let meta = social_post_meta(
        visibility_label(&post.visibility),
        Some(protocol_label(&post.protocol)),
        post.published_at.as_deref(),
        None,
        post.reply_count,
        post.like_count,
        post.boost_count,
    );
    row_with_kind(
        "post",
        &format!("post:{}", post.id),
        title,
        &indicator.explanation,
        &preview_markdown_safe(&post.content),
        &meta,
        &indicator.label,
        indicator.tone,
        "Reply",
        if matches!(post.visibility, Visibility::Public) {
            "Delete"
        } else {
            "Favorite"
        },
    )
}

fn draft_row(draft: &StoredDraft) -> UiRow {
    let title = if draft.text.trim().is_empty() {
        "Untitled draft".to_string()
    } else {
        preview_markdown_safe(&draft.text)
    };
    let indicator = draft_audience_indicator(draft);
    let mut details = vec![format!(
        "{} via {}",
        indicator.explanation,
        protocol_label(&draft.protocol)
    )];
    if !draft.recipients.trim().is_empty() {
        details.push(format!("Recipients: {}", draft.recipients));
    }
    if let Some(reply) = &draft.in_reply_to {
        details.push(format!("Reply to: {reply}"));
    }
    if !draft.attachments.is_empty() {
        details.push(format!("{} media attachment(s)", draft.attachments.len()));
    }
    row(
        &format!("draft:{}", draft.id),
        &title,
        &format!("Updated {}", draft.updated_at),
        &details.join(". "),
        &indicator.label,
        indicator.tone,
        "Open draft",
        "Delete draft",
    )
}

fn compose_audience_summary(compose: &ComposeState, snapshot: &OwnerSnapshotBundle) -> String {
    let route = protocol_label(&compose.protocol);
    let indicator = compose_audience_indicator(compose);
    let base = match compose.visibility {
        Visibility::Public => {
            "Visible to anyone who can read the public web, ActivityPub, or supported public protocol routes.".to_string()
        }
        Visibility::Unlisted => {
            "Visible to addressed/federated audiences, but not promoted as a public timeline post.".to_string()
        }
        Visibility::Followers => {
            "Visible to approved followers/friends on ActivityPub. It should not appear in anonymous public feeds.".to_string()
        }
        Visibility::Direct => {
            if let Some(list_id) = compose.audience_list_id.as_deref().filter(|id| !id.is_empty())
            {
                if let Some(list) = snapshot.audience_lists.iter().find(|list| list.id == list_id)
                {
                    format!(
                        "Visible only to audience group {} ({} member(s)): {}.",
                        list.name,
                        list.member_count,
                        audience_members_preview(list)
                    )
                } else {
                    format!(
                        "Visible only to audience group id {list_id}, if that group exists on the server."
                    )
                }
            } else {
                let recipients = split_list(&compose.recipients);
                if recipients.is_empty() {
                    "Direct post has no recipients yet.".to_string()
                } else {
                    format!("Visible only to {} direct recipient(s).", recipients.len())
                }
            }
        }
    };
    let mut parts = vec![
        format!("Indicator: {}.", indicator.label),
        base,
        format!("Route: {route}."),
    ];
    if compose.encrypt {
        parts.push("Encryption requested.".into());
    }
    if !compose.attachments.is_empty() {
        parts.push(format!(
            "{} media attachment(s) will use their upload access policy.",
            compose.attachments.len()
        ));
    }
    parts.join(" ")
}

fn audience_members_preview(list: &OwnerAudienceList) -> String {
    if list.member_actor_ids.is_empty() {
        return "no members configured".into();
    }
    let preview = list
        .member_actor_ids
        .iter()
        .take(3)
        .map(|member| compact_actor(member))
        .collect::<Vec<_>>()
        .join(", ");
    if list.member_actor_ids.len() > 3 {
        format!("{preview}, and {} more", list.member_actor_ids.len() - 3)
    } else {
        preview
    }
}

fn reply_context_summary(object_id: &str, data: &DeskData) -> String {
    if let Some(detail) = data
        .post_detail
        .as_ref()
        .filter(|detail| detail.post.id == object_id)
    {
        return format!(
            "{} {}",
            detail.post.title.as_deref().unwrap_or("Selected post"),
            preview_markdown_safe(
                detail
                    .content_html
                    .as_deref()
                    .unwrap_or(&detail.post.content)
            )
        );
    }
    data.snapshot
        .home_timeline
        .iter()
        .find(|post| post.object_id == object_id)
        .map(|post| preview_markdown_safe(post.content_html.as_deref().unwrap_or(&post.content)))
        .or_else(|| {
            data.snapshot
                .posts
                .iter()
                .find(|post| post.id == object_id)
                .map(|post| preview_markdown_safe(&post.content))
        })
        .unwrap_or_else(|| format!("Replying to {object_id}. Open context for detail."))
}

fn reply_activity_row(post_id: &str, index: usize, reply: &serde_json::Value) -> UiRow {
    let actor = json_string_any(
        reply,
        &[
            "actor_display_name",
            "actorDisplayName",
            "display_name",
            "displayName",
            "actor_username",
            "actorUsername",
            "actor",
            "attributedTo",
        ],
    )
    .unwrap_or_else(|| "Reply".into());
    let content = json_string_any(
        reply,
        &["content_html", "contentHtml", "content", "summary", "text"],
    )
    .unwrap_or_else(|| "Reply content is available in server detail.".into());
    let published = json_string_any(
        reply,
        &[
            "published_at",
            "publishedAt",
            "published",
            "created_at",
            "createdAt",
        ],
    )
    .unwrap_or_else(|| "reply".into());
    let visibility = json_string_any(reply, &["visibility", "to_visibility", "audience"])
        .unwrap_or_else(|| "unknown".into());
    let (chip, tone, visibility_detail) = if visibility == "unknown" {
        (
            "Reply",
            "info",
            "Reply visibility was not included by the server.".to_string(),
        )
    } else {
        (
            visibility_string_label(&visibility),
            visibility_tone(&visibility),
            visibility_explanation_str(&visibility).to_string(),
        )
    };
    row(
        &format!("post-detail:{post_id}:reply:{index}"),
        &actor,
        &published,
        &format!("{visibility_detail} {}", preview_markdown_safe(&content)),
        chip,
        tone,
        "",
        "",
    )
}

fn notification_action_sentence(kind: &str) -> &'static str {
    match kind {
        "mention" => "Someone mentioned you",
        "reply" => "Someone replied to a post",
        "favourite" | "favorite" | "like" => "Someone liked a post",
        "repost" | "boost" => "Someone boosted a post",
        "follow" => "Someone requested to follow you",
        _ => "A social notification arrived",
    }
}

fn selected_visibility_inspector_rows(selected: &UiRow) -> Vec<UiRow> {
    let Some((label, explanation, tone)) = selected_visibility_context(selected) else {
        return Vec::new();
    };
    vec![row(
        &format!("visibility:{}", selected.id),
        "Who can see this",
        label,
        explanation,
        label,
        tone,
        "",
        "",
    )]
}

fn selected_visibility_context(row: &UiRow) -> Option<(&'static str, &'static str, &'static str)> {
    let chip = row.chip.to_ascii_lowercase();
    if chip.ends_with(" people") || chip == "1 person" {
        return Some((
            "Direct",
            "Only named recipients or the selected audience group should be able to read this.",
            "ok",
        ));
    }
    match chip.as_str() {
        "public" | "public web" => Some((
            "Public web",
            "Anyone who can read the public web, public ActivityPub, or supported public protocol routes may be able to read this.",
            "warn",
        )),
        "unlisted" => Some((
            "Unlisted",
            "People with the link or addressed/federated audiences may be able to read this, but it is not promoted as a public timeline post.",
            "info",
        )),
        "followers" | "private" => Some((
            "Followers",
            "Approved followers or friends are the intended audience. Remote follower servers may receive delivered copies.",
            "ok",
        )),
        "direct" => Some((
            "Direct",
            "Only named recipients or the selected audience group should be able to read this.",
            "ok",
        )),
        "group" => Some((
            "Audience group",
            "Only the selected audience group should be able to read this.",
            "ok",
        )),
        "e2ee" | "e2ee 1:1" | "e2ee group" => Some((
            "Encrypted direct",
            "Encrypted content is intended only for the selected recipient or recipient group.",
            "ok",
        )),
        _ => None,
    }
}

fn json_string_any(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(json_string_value))
}

fn json_string_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        serde_json::Value::Object(map) => map
            .get("name")
            .or_else(|| map.get("id"))
            .and_then(json_string_value),
        _ => None,
    }
}

fn profile_preview_text(profile: &ProfileFormState) -> String {
    let display =
        optional_trimmed(&profile.display_name).unwrap_or_else(|| "(no display name)".into());
    let actor_type = optional_trimmed(&profile.actor_type).unwrap_or_else(|| "Person".into());
    let summary = optional_trimmed(&profile.summary).unwrap_or_else(|| "(no summary)".into());
    let icon = optional_trimmed(&profile.icon).unwrap_or_else(|| "no avatar URL".into());
    let image = optional_trimmed(&profile.image).unwrap_or_else(|| "no header image URL".into());
    format!(
        "Public preview: {display} ({actor_type}). Summary: {}. Avatar: {icon}. Header: {image}.",
        preview_markdown_safe(&summary)
    )
}

fn profile_form_fingerprint(profile: &ProfileFormState) -> String {
    let seed = format!(
        "{}\n{}\n{}\n{}\n{}",
        profile.actor_type.trim(),
        profile.display_name.trim(),
        profile.summary.trim(),
        profile.icon.trim(),
        profile.image.trim()
    );
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn notification_row(notice: &OwnerNotification) -> UiRow {
    let actor = notice
        .actor_display_name
        .as_deref()
        .or(notice.actor_username.as_deref())
        .unwrap_or(&notice.actor_id);
    let action = match notice.kind.as_str() {
        "mention" => "Mentioned",
        "reply" => "Replied",
        "favourite" | "favorite" | "like" => "Liked",
        "repost" | "boost" => "Boosted",
        "follow" => "Follow requested",
        kind => kind,
    };
    let title = format!("{action} • {actor}");
    let context = format!(
        "{} {}",
        notice
            .context_post_visibility
            .as_deref()
            .unwrap_or("related"),
        if notice.context_post_id.is_some() || notice.post_id.is_some() {
            "post"
        } else {
            "notice"
        }
    );
    let detail = notification_preview_detail(notice);
    let unread = !json_truthy(&notice.read);
    let has_context_post = notice.context_post_id.is_some() || notice.post_id.is_some();
    let is_conversational = matches!(notice.kind.as_str(), "mention" | "reply");
    let has_open_link = has_openable_link([
        notice.context_post_content_html.as_deref(),
        notice.context_post_content.as_deref(),
        notice.content.as_deref(),
    ]);
    let (primary, secondary) = if has_context_post {
        if unread {
            if is_conversational {
                ("Mark read", "Reply")
            } else {
                ("Mark read", "Open context")
            }
        } else if is_conversational {
            ("Reply", "Open context")
        } else {
            ("Open context", "")
        }
    } else if has_open_link {
        if unread {
            ("Mark read", "Open link")
        } else {
            ("Open link", "")
        }
    } else if unread {
        ("Mark read", "")
    } else {
        ("", "")
    };
    let subtitle = format!(
        "{} · {}",
        context,
        notice.created_at.as_deref().unwrap_or("notification")
    );
    row_with_kind(
        "notification",
        &format!("notification:{}", notice.id),
        &title,
        subtitle.trim(),
        &detail,
        if unread { "Needs review" } else { "Reviewed" },
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
        primary,
        secondary,
    )
}

fn notification_preview_detail(notice: &OwnerNotification) -> String {
    let visibility = notice
        .context_post_visibility
        .as_deref()
        .map(visibility_explanation_str)
        .unwrap_or("Visibility is not included with this notification.");
    let reply_text = notice.content.as_deref().map(preview_markdown_safe);
    let context_text = notice
        .context_post_content_html
        .as_deref()
        .or(notice.context_post_content.as_deref())
        .map(preview_markdown_safe);

    if matches!(notice.kind.as_str(), "reply" | "mention") {
        return match (reply_text, context_text) {
            (Some(reply), Some(context)) => {
                format!("{visibility} Reply: {reply} Original post: {context}")
            }
            (Some(reply), None) => format!("{visibility} Reply: {reply}"),
            (None, Some(context)) => format!("{visibility} Original post: {context}"),
            (None, None) => {
                format!("{visibility} Open this notice to inspect reply details.")
            }
        };
    }

    let source = context_text
        .or(reply_text)
        .unwrap_or_else(|| "Open this notice to inspect details.".to_string());
    format!("{visibility} {source}")
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
        "",
    )
}

fn follower_row(follower: &OwnerFollower) -> UiRow {
    let status = follower.status.to_ascii_lowercase();
    let status_label = status.clone();
    let (primary, secondary, tone) = match status.as_str() {
        "pending" => ("Approve", "Reject", "warn"),
        "approved" | "accepted" => ("", "Remove", "ok"),
        "rejected" => ("Approve", "", "danger"),
        _ => ("Approve", "Reject", "info"),
    };
    let title = format!("{} follows you", compact_actor(&follower.follower_actor_id));
    let detail = match status.as_str() {
        "pending" => "Review this request before the account can read follower-only posts.",
        "approved" | "accepted" => {
            "Approved follower. They can receive follower-only posts unless removed."
        }
        "rejected" => "Rejected follower. They cannot read follower-only posts through approval.",
        _ => "Follower status is unusual; review before sharing private content.",
    };
    row_with_kind(
        "relationship",
        &format!("follower:{}", follower.follower_actor_id),
        &title,
        "Can read private posts only if approved",
        detail,
        "Inbox details hidden; open Diagnostics for raw delivery data.",
        &status_label,
        tone,
        primary,
        secondary,
    )
}

fn friend_row(friend: &OwnerFriend) -> UiRow {
    let actor = compact_actor(&friend.friend_actor_id);
    row_with_kind(
        "relationship",
        &format!("actor:{}", friend.friend_actor_id),
        &format!("You and {actor} are friends"),
        "Mutual private sharing",
        "Friend means both sides can participate in the private social graph. Manage group membership from Audience Groups.",
        "Owner-only relationship",
        "Friend",
        "ok",
        "Unfriend",
        "Block",
    )
}

fn following_row(following: &OwnerFollowing) -> UiRow {
    let status = following.status.to_ascii_lowercase();
    let status_label = status.clone();
    let (primary, secondary, tone) = match status.as_str() {
        "accepted" | "approved" => ("Unfollow", "", "ok"),
        "pending" => ("Cancel", "", "warn"),
        "failed" => ("Follow", "", "danger"),
        _ => ("Unfollow", "", "info"),
    };
    let actor = compact_actor(&following.target_actor_id);
    let title = match status.as_str() {
        "pending" => format!("Follow request pending for {actor}"),
        "failed" => format!("Follow failed for {actor}"),
        _ => format!("You follow {actor}"),
    };
    row_with_kind(
        "relationship",
        &format!("following:{}", following.target_actor_id),
        &title,
        "Remote relationship may be visible to that server",
        &format!("Follow status: {}", status_label),
        "Follow sends a relationship signal; use Watch for private public-post monitoring.",
        &status_label,
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
    let follow_status = actor
        .following_status
        .as_deref()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let follow_action = match follow_status.as_str() {
        "accepted" | "approved" | "following" => "Unfollow",
        "pending" | "requested" => "Cancel",
        _ => "Follow",
    };
    row(
        &format!("actor:{}", actor.id),
        title,
        actor.handle.as_deref().unwrap_or(&actor.id),
        actor.summary.as_deref().unwrap_or(
            "Discovered account. Follow may notify; Watch reads public posts privately.",
        ),
        &follow_status,
        "info",
        follow_action,
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
    let id = item
        .canonical_url
        .as_deref()
        .map(|url| format!("url:{url}"))
        .unwrap_or_else(|| format!("source-item:{}", item.id));
    let open_link = has_openable_link([
        item.canonical_url.as_deref(),
        item.excerpt.as_deref(),
        Some(&item.title),
    ]);
    row(
        &id,
        &item.title,
        &item.source_type,
        item.excerpt
            .as_deref()
            .or(item.canonical_url.as_deref())
            .unwrap_or("Source item"),
        "Source item",
        "info",
        if open_link { "Open link" } else { "" },
        "",
    )
}

fn reading_source_item_row(item: &SourceItem, subtitle: &str, chip: &str) -> UiRow {
    let id = item
        .canonical_url
        .as_deref()
        .map(|url| format!("url:{url}"))
        .unwrap_or_else(|| format!("source-item:{}", item.id));
    let open_link = has_openable_link([
        item.canonical_url.as_deref(),
        item.excerpt.as_deref(),
        Some(&item.title),
    ]);
    row_with_kind(
        "post",
        &id,
        &item.title,
        subtitle,
        item.excerpt
            .as_deref()
            .or(item.canonical_url.as_deref())
            .unwrap_or("Public reading item"),
        &format!("{} · owner-only reader", item.source_type),
        chip,
        "info",
        if open_link { "Open link" } else { "" },
        "",
    )
}

fn search_source_item_row(item: &dais_client_core::OwnerSearchSourceItem) -> UiRow {
    let id = item
        .canonical_url
        .as_deref()
        .map(|url| format!("url:{url}"))
        .unwrap_or_else(|| format!("source-item:{}", item.id));
    let open_link = has_openable_link([
        item.canonical_url.as_deref(),
        item.excerpt.as_deref(),
        Some(&item.title),
    ]);
    row(
        &id,
        &item.title,
        &item.source_type,
        item.excerpt
            .as_deref()
            .or(item.canonical_url.as_deref())
            .unwrap_or("Search source item"),
        "Source item",
        "info",
        if open_link { "Open link" } else { "" },
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
        "Use in compose",
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
        "",
        "Copy evidence",
    )
}

fn delivery_attention_row(delivery: &OwnerDelivery) -> UiRow {
    let mut row = delivery_row(delivery);
    if delivery.status == "failed" {
        row.primary = s("Retry delivery");
        row.secondary = s("Inspect delivery");
    }
    row
}

fn delivery_row(delivery: &OwnerDelivery) -> UiRow {
    let (primary, secondary) = delivery_action_pair(delivery.status.as_str());
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
        delivery_tone(delivery.status.as_str()),
        primary,
        secondary,
    )
}

fn delivery_action_pair(status: &str) -> (&'static str, &'static str) {
    match status {
        "failed" | "retry" => ("Retry delivery", "Cancel delivery"),
        "queued" => ("Cancel delivery", "Inspect delivery"),
        "delivered" => ("Open context", ""),
        _ => ("Inspect delivery", "Open context"),
    }
}

fn delivery_tone(status: &str) -> &'static str {
    match status {
        "failed" => "danger",
        "delivered" => "ok",
        "retry" | "queued" => "warn",
        _ => "info",
    }
}

fn delivery_failure_explanation(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let likely = if lower.contains("timeout") || lower.contains("timed out") {
        "The remote server did not answer in time. Retry is usually reasonable."
    } else if lower.contains("401") || lower.contains("403") || lower.contains("authorized") {
        "The remote server rejected access or signing. Check authorized-fetch and key configuration before retrying."
    } else if lower.contains("404") || lower.contains("410") {
        "The remote target may no longer exist. Retrying is unlikely to help unless the target URL changed."
    } else if lower.contains("429") || lower.contains("rate") {
        "The remote server is rate limiting delivery. Wait before retrying."
    } else if lower.contains("5") || lower.contains("unavailable") || lower.contains("bad gateway")
    {
        "The remote server appears unhealthy. Retry later or inspect the remote status."
    } else {
        "Delivery failed before the remote server confirmed receipt. Retry if the target is still expected to receive this activity."
    };
    format!("{likely} Raw error: {}", clean_text(error))
}

fn moderation_reply_row(reply: &ModerationReplyRow) -> UiRow {
    let raw_status = reply.moderation_status.as_deref().unwrap_or("needs review");
    let normalized_status = raw_status.trim().to_ascii_lowercase().replace(' ', "_");
    let status = humanize_status(raw_status);
    let hidden =
        json_truthy(&reply.hidden) || matches!(normalized_status.as_str(), "hidden" | "rejected");
    let flags = if reply.moderation_flags.is_empty() {
        String::new()
    } else {
        format!("Flags: {}. ", reply.moderation_flags.join(", "))
    };
    let score = reply
        .moderation_score
        .map(|score| format!("Advisory score: {:.2}. ", score))
        .unwrap_or_default();
    let (primary, secondary) = moderation_reply_actions(&normalized_status, hidden);
    let (chip, tone) = if hidden {
        if normalized_status == "rejected" {
            ("Rejected", "danger")
        } else {
            ("Hidden", "warn")
        }
    } else if normalized_status == "approved" {
        ("Approved", "ok")
    } else if reply.moderation_flags.is_empty() {
        ("Review", "warn")
    } else {
        ("Flagged", "danger")
    };
    row(
        &format!("moderation-reply:{}", reply.id),
        reply
            .actor_display_name
            .as_deref()
            .or(reply.actor_username.as_deref())
            .unwrap_or(&reply.actor_id),
        &status,
        &format!("{flags}{score}{}", preview_markdown_safe(&reply.content)),
        chip,
        tone,
        primary,
        secondary,
    )
}

fn moderation_reply_actions(status: &str, hidden: bool) -> (&'static str, &'static str) {
    match (status, hidden) {
        ("approved", false) => ("Hide reply", "Reject reply"),
        ("rejected", _) => ("Approve reply", ""),
        ("hidden", _) | (_, true) => ("Approve reply", "Reject reply"),
        _ => ("Approve reply", "Hide reply"),
    }
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
                } else if is_html_boundary_tag(&lower) {
                    append_text_boundary(&mut output);
                }
                in_tag = false;
                tag.clear();
            }
            _ if in_tag => tag.push(ch),
            _ => output.push(ch),
        }
    }
    output
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| ch == '|' || ch.is_whitespace())
        .trim()
        .to_string()
}

fn is_html_boundary_tag(tag: &str) -> bool {
    let tag = tag.trim_start_matches('/');
    tag.starts_with("br")
        || tag.starts_with('p')
        || tag.starts_with("div")
        || tag.starts_with("blockquote")
        || tag.starts_with("li")
        || tag.starts_with("ul")
        || tag.starts_with("ol")
}

fn append_text_boundary(output: &mut String) {
    if output.trim().is_empty() {
        return;
    }
    let trimmed = output.trim_end();
    if trimmed.ends_with('.')
        || trimmed.ends_with('!')
        || trimmed.ends_with('?')
        || trimmed.ends_with('|')
    {
        output.push(' ');
    } else {
        output.push_str(" | ");
    }
}

fn preview_markdown_safe(value: &str) -> String {
    const MAX_CHARS: usize = 220;
    let clean = clean_text(value).replace("  ", " ").trim().to_string();
    if clean.chars().count() > MAX_CHARS {
        let clipped: String = clean.chars().take(MAX_CHARS).collect();
        format!("{clipped}…")
    } else {
        clean
    }
}

fn humanize_status(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

fn resolve_external_url(controller: &DeskController, row_id: &str) -> Result<String, String> {
    let source = controller
        .find_row(row_id)
        .and_then(|row| {
            extract_first_url(&row.detail)
                .or_else(|| extract_first_url(&row.title))
                .or_else(|| extract_first_url(&row.subtitle))
        })
        .or_else(|| row_id.strip_prefix("url:").map(ToOwned::to_owned))
        .or_else(|| row_id.strip_prefix("media:").map(ToOwned::to_owned))
        .or_else(|| {
            delivery_id_from_row(row_id).and_then(|id| {
                controller
                    .data
                    .deliveries
                    .iter()
                    .find(|delivery| delivery.id == id)
                    .map(|delivery| delivery.target_url.clone())
            })
        })
        .or_else(|| {
            matches!(row_id, "identity:profile" | "health:profile")
                .then(|| controller.data.snapshot.profile.actor_url.clone())
        });
    let normalized = source
        .filter(|candidate| candidate.starts_with("http://") || candidate.starts_with("https://"))
        .ok_or_else(|| "no external URL on this item".to_string())?;
    Ok(normalized)
}

fn extract_first_url(value: &str) -> Option<String> {
    if let Some(url) = extract_url_from_href(value) {
        return Some(url);
    }
    extract_url_from_markdown(value).or_else(|| extract_url_from_plain_text(value))
}

fn extract_url_from_href(value: &str) -> Option<String> {
    let value_lower = value.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(found) = value_lower[cursor..].find("href") {
        let href_pos = cursor + found;
        let mut idx = href_pos + 4;
        let bytes = value.as_bytes();
        while idx < bytes.len() && value.as_bytes()[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() || bytes[idx] != b'=' {
            cursor = href_pos + 4;
            continue;
        }
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        let (start, end) = if bytes[idx] == b'"' || bytes[idx] == b'\'' {
            let quote = bytes[idx];
            let start = idx + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != quote {
                end += 1;
            }
            if end >= bytes.len() {
                break;
            }
            (start, end)
        } else {
            let start = idx;
            let mut end = idx;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                if bytes[end] == b'>' || bytes[end] == b'/' {
                    break;
                }
                end += 1;
            }
            (start, end)
        };
        if let Some(url) = clean_url_candidate(&value[start..end]) {
            return Some(url);
        }
        cursor = end + 1;
    }
    None
}

fn extract_url_from_markdown(value: &str) -> Option<String> {
    let mut pos = 0;
    let bytes = value.as_bytes();
    while let Some(open_paren) = value[pos..].find('(') {
        let start = pos + open_paren + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b')' && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        if end > start && bytes[end.saturating_sub(1)] != b'(' {
            let candidate = &value[start..end];
            if let Some(url) = clean_url_candidate(candidate) {
                return Some(url);
            }
        }
        pos = end.saturating_add(1);
        if pos > bytes.len() {
            break;
        }
    }
    None
}

fn extract_url_from_plain_text(value: &str) -> Option<String> {
    for prefix in ["https://", "http://"] {
        let mut pos = 0;
        while let Some(found) = value[pos..].find(prefix) {
            let start = pos + found;
            let bytes = value.as_bytes();
            let mut end = start + prefix.len();
            while end < bytes.len() && is_url_text_byte(bytes[end]) {
                end += 1;
            }
            if let Some(url) = clean_url_candidate(&value[start..end]) {
                return Some(url);
            }
            pos = end + 1;
        }
    }
    None
}

fn clean_url_candidate(value: &str) -> Option<String> {
    let candidate = value
        .trim()
        .trim_start_matches(|ch| ch == '\'' || ch == '"')
        .trim_end_matches(|ch| {
            ch == '\'' || ch == '"' || ch == ',' || ch == '.' || ch == ')' || ch == ']' || ch == '>'
        });
    if candidate.starts_with("http://") || candidate.starts_with("https://") {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn is_url_text_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b':'
            | b'/'
            | b'?'
            | b'#'
            | b'['
            | b']'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b'%'
    )
}

fn has_openable_link(parts: [Option<&str>; 3]) -> bool {
    parts
        .into_iter()
        .filter_map(|value| value)
        .any(|value| extract_first_url(value).is_some())
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

fn choose_media_file_path() -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(r#"POSIX path of (choose file with prompt "Choose media to upload")"#)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("-128") || stderr.to_ascii_lowercase().contains("cancel") {
                return Ok(None);
            }
            return Err(stderr.trim().if_empty("native file chooser failed"));
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("native media chooser is not implemented on this platform; paste a local file path instead".into())
    }
}

fn object_id_from_row(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("post:")
        .or_else(|| row_id.strip_prefix("timeline:"))
        .or_else(|| row_id.strip_prefix("post-detail:"))
        .or_else(|| row_id.strip_prefix("url:"))
}

fn notification_id_from_row(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("notification:")
        .or_else(|| row_id.strip_prefix("notification-detail:"))
        .or_else(|| row_id.strip_prefix("notification-context:"))
}

fn delivery_id_from_row(row_id: &str) -> Option<&str> {
    row_id
        .strip_prefix("delivery:")
        .or_else(|| row_id.strip_prefix("delivery-detail:"))
        .or_else(|| row_id.strip_prefix("delivery-failure:"))
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

fn visibility_explanation_str(visibility: &str) -> &'static str {
    match visibility.trim().to_ascii_lowercase().as_str() {
        "public" => "Visibility: public; anyone may be able to read it.",
        "unlisted" => {
            "Visibility: unlisted; not promoted publicly, but may be visible outside friends."
        }
        "followers" | "private" => {
            "Visibility: followers/friends; intended for approved followers, not anonymous public readers."
        }
        "direct" => "Visibility: direct/select; intended only for named recipients or a selected group.",
        _ => "Visibility: unknown; the server did not provide a precise audience.",
    }
}

fn visibility_from_value(value: &str) -> Option<Visibility> {
    match value.trim().to_ascii_lowercase().as_str() {
        "public" => Some(Visibility::Public),
        "unlisted" => Some(Visibility::Unlisted),
        "direct" => Some(Visibility::Direct),
        "followers" | "private" => Some(Visibility::Followers),
        _ => None,
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

fn protocol_label(protocol: &ProtocolRoute) -> &'static str {
    match protocol {
        ProtocolRoute::ActivityPub => "ActivityPub",
        ProtocolRoute::AtProto => "Bluesky",
        ProtocolRoute::Both => "Both",
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn protocol_from_value(value: &str) -> Option<ProtocolRoute> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', '_'], "");
    match normalized.as_str() {
        "activitypub" => Some(ProtocolRoute::ActivityPub),
        "atproto" | "bluesky" => Some(ProtocolRoute::AtProto),
        "both" => Some(ProtocolRoute::Both),
        _ => None,
    }
}

fn compose_warning(compose: &ComposeState) -> String {
    if compose.text.trim().is_empty() {
        return "Write a post before sending.".into();
    }
    if matches!(compose.visibility, Visibility::Direct)
        && split_list(&compose.recipients).is_empty()
        && compose.audience_list_id.as_deref().unwrap_or("").is_empty()
    {
        return "Direct posts require named recipients or an audience group.".into();
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
            || !split_list(&compose.recipients).is_empty()
            || !compose.audience_list_id.as_deref().unwrap_or("").is_empty())
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

fn compact_actor(actor: &str) -> String {
    compact_url(actor)
        .replace("/users/", "/@")
        .replace("/profile/", "/@")
}

fn infer_watch_type(target: &str) -> &'static str {
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("at://") || lower.contains("bsky.app/profile/") && lower.contains("/post/")
    {
        "bluesky_post"
    } else if lower.contains("bsky.app/profile/")
        || lower.ends_with(".bsky.social")
        || (!target.starts_with("http") && target.contains('.') && !target.contains('@'))
    {
        "bluesky_actor"
    } else if lower.ends_with(".atom") || lower.contains("atom.xml") {
        "atom"
    } else if lower.ends_with(".rss") || lower.ends_with(".xml") || lower.contains("/rss") {
        "rss"
    } else if lower.contains("/statuses/")
        || lower.contains("/objects/")
        || lower.contains("/notes/")
        || lower.contains("/@")
            && lower
                .rsplit('/')
                .next()
                .is_some_and(|part| part.chars().any(|c| c.is_ascii_digit()))
    {
        "activitypub_object"
    } else {
        "activitypub_actor"
    }
}

fn media_type_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn attachment_url(value: &serde_json::Value) -> Option<String> {
    value
        .get("url")
        .or_else(|| value.get("href"))
        .or_else(|| value.get("remote_url"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn attachment_media_type(value: &serde_json::Value) -> Option<String> {
    value
        .get("mediaType")
        .or_else(|| value.get("media_type"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn parse_u16(value: &str, default: Option<u16>) -> Option<u16> {
    optional_trimmed(value)
        .and_then(|value| value.parse::<u16>().ok())
        .or(default)
}

fn parse_u64(value: &str, default: u64) -> u64 {
    optional_trimmed(value)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn optional_filter(value: &str, ignored: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(ignored) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn audience_form_from_list(list: &OwnerAudienceList) -> AudienceFormState {
    AudienceFormState {
        id: list.id.clone(),
        name: list.name.clone(),
        description: list.description.clone().unwrap_or_default(),
        categories: list.allowed_categories.join(", "),
        members: list.member_actor_ids.join(", "),
    }
}

fn fixture_post_detail(object_id: &str, snapshot: &OwnerSnapshotBundle) -> Option<OwnerPostDetail> {
    snapshot
        .posts
        .iter()
        .find(|post| post.id == object_id)
        .cloned()
        .or_else(|| {
            snapshot
                .home_timeline
                .iter()
                .find(|post| post.object_id == object_id)
                .map(|post| OwnerPost {
                    id: post.object_id.clone(),
                    title: post.actor_display_name.clone(),
                    content: post.content.clone(),
                    visibility: Visibility::Followers,
                    protocol: ProtocolRoute::ActivityPub,
                    encrypted: false,
                    attachments: Vec::new(),
                    reply_count: post.reply_count,
                    like_count: post.like_count,
                    boost_count: post.boost_count,
                    published_at: post.published_at.clone(),
                })
        })
        .map(|post| OwnerPostDetail {
            post,
            content_html: Some("<p>Preview thread detail.</p>".into()),
            in_reply_to: None,
            replies: vec![serde_json::json!({"id": "fixture-reply"})],
            likes: vec![serde_json::json!({"id": "fixture-like"})],
            boosts: Vec::new(),
        })
}

fn fixture_data(api_error: Option<String>) -> DeskData {
    let settings = StoredOwnerSettings::default();
    let snapshot = local_snapshot(settings, api_error.clone()).into();
    DeskData {
        snapshot,
        post_detail: None,
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
            items: vec![SourceItem {
                id: "watch-item-nobel".into(),
                title: "Nobel Prize public update".into(),
                source_type: "activitypub".into(),
                canonical_url: Some("https://social.example/users/nobel/posts/1".into()),
                excerpt: Some(
                    "A watched public account posted a research prize announcement.".into(),
                ),
                rights_policy_json: "{\"excerpt_only\":true,\"private_reader_only\":true}".into(),
                read: false,
            }],
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

fn drafts_path_for_settings(settings_path: &Path) -> PathBuf {
    let mut path = settings_path.to_path_buf();
    path.set_file_name("owner-drafts.json");
    path
}

fn load_drafts_from(path: &PathBuf) -> Result<StoredDrafts, String> {
    if !path.exists() {
        return Ok(StoredDrafts::default());
    }
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let drafts: StoredDrafts = serde_json::from_str(&json).map_err(|error| error.to_string())?;
    Ok(drafts)
}

fn persist_drafts_to(path: &PathBuf, drafts: StoredDrafts) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&drafts).map_err(|error| error.to_string())?;
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

fn draft_id_for(account_id: &str, updated_at: &str, text: &str) -> String {
    let seed = format!("{account_id}:{updated_at}:{text}");
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("draft-{:x}", hasher.finish())
}

fn unix_timestamp_label() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
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

    fn is_supported_row_action(action: &str) -> bool {
        matches!(
            action,
            "" | "Reply"
                | "Favorite"
                | "Boost"
                | "Repost"
                | "Delete"
                | "Switch"
                | "Validate token"
                | "Mark read"
                | "Approve"
                | "Reject"
                | "Remove"
                | "Follow"
                | "Unfollow"
                | "Cancel"
                | "Unfriend"
                | "Watch"
                | "Stop watching"
                | "Refresh"
                | "Retry delivery"
                | "Cancel delivery"
                | "Approve reply"
                | "Hide reply"
                | "Reject reply"
                | "Block"
                | "Unblock"
                | "Open original"
                | "Open link"
                | "Open context"
                | "Find people"
                | "Add Watch"
                | "Inspect delivery"
                | "Copy evidence"
                | "Revoke media"
                | "Use in compose"
                | "Save draft"
                | "Open draft"
                | "Delete draft"
        )
    }

    fn assert_supported_row_actions(rows: &[UiRow]) {
        for row in rows {
            assert!(
                is_supported_row_action(&row.primary),
                "unexpected primary action '{}' on row {}",
                row.primary,
                row.id
            );
            assert!(
                is_supported_row_action(&row.secondary),
                "unexpected secondary action '{}' on row {}",
                row.secondary,
                row.id
            );
        }
    }

    #[test]
    fn fixture_rows_only_include_supported_primary_secondary_actions() {
        let mut controller = DeskController::fixture_for_tests();
        for screen in &[
            "today",
            "inbox",
            "compose",
            "posts",
            "saved",
            "find",
            "relationship",
            "friends",
            "followers",
            "following",
            "watches",
            "audience",
            "blocks",
            "health",
            "deliveries",
            "moderation",
            "identity",
            "settings",
            "stats",
            "accounts",
        ] {
            controller.select_screen(screen);
            assert_supported_row_actions(&controller.rows_for_active_screen());
            if let Some(first_row) = controller.rows_for_active_screen().first() {
                controller.select_row(first_row.id.as_str());
                assert_supported_row_actions(&controller.inspector_rows(first_row.id.as_str()));
            } else {
                panic!("screen {screen} has no rows");
            }
        }
        controller.select_row("post:fixture-private-post");
        assert_supported_row_actions(&controller.inspector_rows("post:fixture-private-post"));
    }

    #[test]
    fn empty_people_screens_show_next_step_guidance() {
        let mut controller = DeskController::fixture_for_tests();
        controller.data.snapshot.friends.clear();
        controller.data.snapshot.followers.clear();
        controller.data.snapshot.following.clear();
        controller.data.watches.subscriptions.clear();
        controller.data.sources.subscriptions.clear();
        controller.data.watches.items.clear();
        controller.data.sources.items.clear();
        controller.data.snapshot.audience_lists.clear();
        controller.data.snapshot.moderation.blocks.clear();
        controller.data.snapshot.moderation.allowlist.clear();

        controller.select_screen("relationship");
        let relationships = controller.rows_for_active_screen();
        assert_eq!(relationships[0].id.as_str(), "relationship:empty");
        assert_eq!(relationships[0].primary.as_str(), "Find people");

        controller.select_screen("watches");
        let watches = controller.rows_for_active_screen();
        assert_eq!(watches[0].id.as_str(), "watches:empty");
        assert_eq!(watches[0].primary.as_str(), "Add Watch");

        controller.select_screen("audience");
        let audience = controller.rows_for_active_screen();
        assert_eq!(audience[0].id.as_str(), "audience:empty");
        assert!(audience[0].detail.contains("small, intentional sharing"));
    }

    #[test]
    fn strips_markup_and_script_content() {
        let cleaned = clean_text("<p>Hello <b>friend</b><script>alert(1)</script></p>");
        assert_eq!(cleaned, "Hello friend");
    }

    #[test]
    fn safe_text_preserves_readable_block_boundaries() {
        let cleaned = clean_text(
            "<p>First paragraph</p><blockquote>Quoted text</blockquote><ul><li>One</li><li>Two</li></ul>",
        );
        assert_eq!(cleaned, "First paragraph | Quoted text | One | Two");
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
    fn normalizes_follower_status_when_rendering_actions() {
        let pending = follower_row(&OwnerFollower {
            id: "1".into(),
            actor_id: "me".into(),
            follower_actor_id: "them".into(),
            follower_inbox: "https://example.test/inbox".into(),
            follower_shared_inbox: None,
            status: "Approved".into(),
            created_at: None,
            updated_at: None,
        });
        assert_eq!(pending.primary.as_str(), "");
        assert_eq!(pending.secondary.as_str(), "Remove");
        assert_eq!(pending.chip.as_str(), "approved");
    }

    #[test]
    fn discovered_actor_primary_action_reflects_follow_state() {
        let pending = discovered_actor_row(&OwnerDiscoveredActor {
            id: "actor".into(),
            actor_type: None,
            inbox: "https://example.test/inbox".into(),
            shared_inbox: None,
            preferred_username: Some("friend".into()),
            name: Some("Friend Name".into()),
            summary: None,
            url: None,
            icon_url: None,
            handle: Some("@friend@example.test".into()),
            following_status: Some("pending".into()),
            target_public_post: None,
            recent_public_posts: Vec::new(),
        });
        assert_eq!(pending.primary.as_str(), "Cancel");
        assert_eq!(pending.chip.as_str(), "pending");
        assert_eq!(pending.secondary.as_str(), "Watch");
    }

    #[test]
    fn friend_rows_expose_unfriend_action() {
        let row = friend_row(&OwnerFriend {
            friend_actor_id: "https://friend.example/users/alice".into(),
            friend_inbox: Some("https://friend.example/inbox".into()),
            friend_shared_inbox: None,
            follower_since: Some("yesterday".into()),
            following_since: Some("yesterday".into()),
            accepted_at: Some("today".into()),
        });
        assert_eq!(row.kind.as_str(), "relationship");
        assert!(row.title.contains("alice"));
        assert_eq!(row.primary.as_str(), "Unfriend");
        assert_eq!(row.secondary.as_str(), "Block");
    }

    #[test]
    fn social_rows_are_typed_cards_with_readable_metadata() {
        let timeline = timeline_row(&OwnerTimelinePost {
            id: "timeline-1".into(),
            object_id: "https://remote.example/posts/1".into(),
            actor_id: "https://remote.example/users/ada".into(),
            actor_username: Some("@ada@remote.example".into()),
            actor_display_name: Some("Ada".into()),
            actor_avatar_url: None,
            content: "Hello from the wider fediverse".into(),
            content_html: Some("<p>Hello from the <strong>wider</strong> fediverse</p>".into()),
            visibility: "public".into(),
            in_reply_to: None,
            published_at: Some("today".into()),
            protocol: Some("ActivityPub".into()),
            reply_count: 2,
            like_count: 1,
            boost_count: 0,
        });
        assert_eq!(timeline.kind.as_str(), "post");
        assert!(timeline.meta.contains("Public"));
        assert!(timeline.meta.contains("ActivityPub"));
        assert!(timeline.meta.contains("2 replies"));
        assert!(timeline.detail.contains("Hello from the wider fediverse"));

        let own = post_row(&OwnerPost {
            id: "post-1".into(),
            title: Some("Own post".into()),
            content: "Private update".into(),
            visibility: Visibility::Followers,
            protocol: ProtocolRoute::ActivityPub,
            encrypted: false,
            attachments: vec![],
            reply_count: 0,
            like_count: 0,
            boost_count: 0,
            published_at: Some("today".into()),
        });
        assert_eq!(own.kind.as_str(), "post");
        assert!(own.subtitle.contains("approved followers"));
        assert!(own.meta.contains("Followers"));
    }

    #[test]
    fn relationship_rows_are_human_readable_and_hide_inboxes() {
        let pending = follower_row(&OwnerFollower {
            id: "f1".into(),
            actor_id: "https://dais.social/users/social".into(),
            follower_actor_id: "https://social.example/users/bob".into(),
            follower_inbox: "https://social.example/inbox".into(),
            follower_shared_inbox: Some("https://social.example/shared".into()),
            status: "pending".into(),
            created_at: Some("yesterday".into()),
            updated_at: Some("today".into()),
        });
        assert_eq!(pending.kind.as_str(), "relationship");
        assert!(pending.title.contains("bob follows you"));
        assert!(pending.detail.contains("Review this request"));
        assert!(!pending.detail.contains("https://social.example/inbox"));
        assert_eq!(pending.primary.as_str(), "Approve");
        assert_eq!(pending.secondary.as_str(), "Reject");

        let following = following_row(&OwnerFollowing {
            id: "follow-1".into(),
            actor_id: "https://dais.social/users/social".into(),
            target_actor_id: "https://news.example/users/editor".into(),
            target_inbox: "https://news.example/inbox".into(),
            status: "accepted".into(),
            created_at: Some("yesterday".into()),
            accepted_at: Some("today".into()),
        });
        assert_eq!(following.kind.as_str(), "relationship");
        assert!(following.title.contains("You follow"));
        assert!(following.title.contains("editor"));
        assert!(following.meta.contains("relationship signal"));
    }

    #[test]
    fn selected_actor_rows_expose_profile_navigation() {
        let mut controller = DeskController::fixture_for_tests();
        controller
            .data
            .snapshot
            .home_timeline
            .push(OwnerTimelinePost {
                id: "timeline-actor".into(),
                object_id: "https://remote.example/posts/actor".into(),
                actor_id: "https://remote.example/users/ada".into(),
                actor_username: Some("@ada@remote.example".into()),
                actor_display_name: Some("Ada".into()),
                actor_avatar_url: None,
                content: "Public note".into(),
                content_html: None,
                visibility: "public".into(),
                in_reply_to: None,
                published_at: Some("today".into()),
                protocol: Some("ActivityPub".into()),
                reply_count: 0,
                like_count: 0,
                boost_count: 0,
            });
        let rows = controller.inspector_rows("timeline:https://remote.example/posts/actor");
        let profile = rows
            .iter()
            .find(|row| row.id.as_str() == "url:https://remote.example/users/ada")
            .expect("author profile row");
        assert_eq!(profile.title.as_str(), "Author profile");
        assert_eq!(profile.primary.as_str(), "Open original");
        assert_eq!(profile.secondary.as_str(), "Watch");

        let friend_rows = controller.inspector_rows("actor:https://friend.example/users/alice");
        assert!(friend_rows
            .iter()
            .any(|row| row.title.as_str() == "Friend profile"
                && row.primary.as_str() == "Open original"));
    }

    #[test]
    fn source_item_row_without_link_has_no_open_action() {
        let row = source_item_row(&SourceItem {
            id: "item-1".into(),
            title: "No URL item".into(),
            source_type: "rss".into(),
            canonical_url: None,
            excerpt: Some("No external link in this excerpt.".into()),
            rights_policy_json: "{}".into(),
            read: false,
        });
        assert_eq!(row.primary.as_str(), "");
    }

    #[test]
    fn source_item_row_with_excerpt_link_keeps_open_action() {
        let row = source_item_row(&SourceItem {
            id: "item-2".into(),
            title: "Excerpt link item".into(),
            source_type: "rss".into(),
            canonical_url: None,
            excerpt: Some("See https://example.org/article for details.".into()),
            rights_policy_json: "{}".into(),
            read: false,
        });
        assert_eq!(row.primary.as_str(), "Open link");
    }

    #[test]
    fn source_item_row_with_title_link_keeps_open_action() {
        let row = source_item_row(&SourceItem {
            id: "item-3".into(),
            title: "https://example.org from title".into(),
            source_type: "rss".into(),
            canonical_url: None,
            excerpt: None,
            rights_policy_json: "{}".into(),
            read: false,
        });
        assert_eq!(row.primary.as_str(), "Open link");
    }

    #[test]
    fn search_source_item_row_without_link_has_no_open_action() {
        let row = search_source_item_row(&dais_client_core::OwnerSearchSourceItem {
            id: "search-item-1".into(),
            source_id: "source-id".into(),
            source_type: "rss".into(),
            title: "No URL search result".into(),
            canonical_url: None,
            excerpt: None,
            published_at: None,
            read: serde_json::json!(false),
            rights_policy_json: "{}".into(),
            created_at: None,
        });
        assert_eq!(row.primary.as_str(), "");
    }

    #[test]
    fn search_source_item_row_with_url_keeps_open_action() {
        let row = search_source_item_row(&dais_client_core::OwnerSearchSourceItem {
            id: "search-item-2".into(),
            source_id: "source-id".into(),
            source_type: "rss".into(),
            title: "Search item with url".into(),
            canonical_url: Some("https://example.org/search".into()),
            excerpt: None,
            published_at: None,
            read: serde_json::json!(false),
            rights_policy_json: "{}".into(),
            created_at: None,
        });
        assert_eq!(row.primary.as_str(), "Open link");
    }

    #[test]
    fn search_source_item_row_with_title_link_keeps_open_action() {
        let row = search_source_item_row(&dais_client_core::OwnerSearchSourceItem {
            id: "search-item-3".into(),
            source_id: "source-id".into(),
            source_type: "rss".into(),
            title: "Check https://search.example/entry".into(),
            canonical_url: None,
            excerpt: None,
            published_at: None,
            read: serde_json::json!(false),
            rights_policy_json: "{}".into(),
            created_at: None,
        });
        assert_eq!(row.primary.as_str(), "Open link");
    }

    #[test]
    fn resolve_external_url_uses_row_title_when_detail_has_no_url() {
        let mut controller = DeskController::fixture_for_tests();
        controller.data.sources.items.push(SourceItem {
            id: "source-item-title-only".into(),
            title: "https://title-only.example/article".into(),
            source_type: "rss".into(),
            canonical_url: None,
            excerpt: None,
            rights_policy_json: "{}".into(),
            read: false,
        });
        controller.select_screen("watches");
        let row_id = "source-item:source-item-title-only";
        let url = resolve_external_url(&controller, row_id).expect("row url");
        assert_eq!(url, "https://title-only.example/article");
    }

    #[test]
    fn dm_rows_only_allow_reply_action() {
        let row = dm_row(&OwnerDirectMessage {
            id: "dm".into(),
            conversation_id: "conv".into(),
            sender_id: "https://friend.example/users/ada".into(),
            content: "hello".into(),
            published_at: "now".into(),
            created_at: Some("now".into()),
        });
        assert_eq!(row.primary.as_str(), "Reply");
        assert_eq!(row.secondary.as_str(), "");
    }

    #[test]
    fn notification_rows_expose_contextual_actions() {
        let reply = notification_row(&OwnerNotification {
            id: "n1".into(),
            kind: "reply".into(),
            actor_id: "https://friend.example/users/ada".into(),
            actor_username: Some("ada".into()),
            actor_display_name: Some("Ada".into()),
            actor_avatar_url: None,
            post_id: Some("post-id".into()),
            activity_id: None,
            content: Some("reply content".into()),
            read: serde_json::Value::Bool(false),
            created_at: Some("1m".into()),
            context_post_id: None,
            context_post_content: Some("context".into()),
            context_post_content_html: None,
            context_post_visibility: None,
            context_post_protocol: None,
            context_post_published_at: None,
        });
        assert_eq!(reply.kind.as_str(), "notification");
        assert_eq!(reply.primary.as_str(), "Mark read");
        assert_eq!(reply.secondary.as_str(), "Reply");

        let read_like = notification_row(&OwnerNotification {
            id: "n2".into(),
            kind: "like".into(),
            actor_id: "https://friend.example/users/ada".into(),
            actor_username: Some("ada".into()),
            actor_display_name: Some("Ada".into()),
            actor_avatar_url: None,
            post_id: Some("post-id".into()),
            activity_id: None,
            content: Some("liked it".into()),
            read: serde_json::Value::Bool(true),
            created_at: Some("1m".into()),
            context_post_id: None,
            context_post_content: Some("context".into()),
            context_post_content_html: None,
            context_post_visibility: None,
            context_post_protocol: None,
            context_post_published_at: None,
        });
        assert_eq!(read_like.kind.as_str(), "notification");
        assert_eq!(read_like.primary.as_str(), "Open context");
        assert_eq!(read_like.secondary.as_str(), "");
    }

    #[test]
    fn notification_row_provides_readable_preview() {
        let row = notification_row(&OwnerNotification {
            id: "n3".into(),
            kind: "like".into(),
            actor_id: "https://social.example/users/vera".into(),
            actor_username: Some("vera".into()),
            actor_display_name: Some("Vera".into()),
            actor_avatar_url: None,
            post_id: None,
            activity_id: None,
            content: Some(
                "<p><a href=\"https://social.example/p/1\">Liked</a> your post.</p>".into(),
            ),
            read: serde_json::Value::Bool(false),
            created_at: Some("2m".into()),
            context_post_id: None,
            context_post_content: None,
            context_post_content_html: None,
            context_post_visibility: None,
            context_post_protocol: None,
            context_post_published_at: None,
        });
        assert!(row
            .detail
            .starts_with("Visibility is not included with this notification."));
        assert!(row.detail.contains("Liked your post."));
        assert_eq!(row.primary, "Mark read");
        assert_eq!(row.secondary, "Open link");
    }

    #[test]
    fn notification_row_from_html_exposes_link_action() {
        let row = notification_row(&OwnerNotification {
            id: "n4".into(),
            kind: "favourite".into(),
            actor_id: "https://social.example/users/leo".into(),
            actor_username: Some("leo".into()),
            actor_display_name: Some("Leo".into()),
            actor_avatar_url: None,
            post_id: None,
            activity_id: None,
            content: Some(
                "<p><a href=\"https://social.example/posts/visible\">View thread</a> to compare.</p>"
                    .into(),
            ),
            read: serde_json::Value::Bool(true),
            created_at: Some("5m".into()),
            context_post_id: None,
            context_post_content: None,
            context_post_content_html: None,
            context_post_visibility: None,
            context_post_protocol: None,
            context_post_published_at: None,
        });
        assert_eq!(row.primary, "Open link");
        assert_eq!(row.secondary, "");
        assert!(row.detail.contains("View thread to compare."));
    }

    #[test]
    fn social_rows_explain_post_visibility() {
        let own = post_row(&OwnerPost {
            id: "p-direct".into(),
            title: Some("Direct note".into()),
            content: "Private detail".into(),
            visibility: Visibility::Direct,
            protocol: ProtocolRoute::ActivityPub,
            encrypted: false,
            attachments: Vec::new(),
            reply_count: 0,
            like_count: 0,
            boost_count: 0,
            published_at: None,
        });
        assert_eq!(own.chip.as_str(), "Direct");
        assert!(own.subtitle.contains("named recipients"));

        let timeline = timeline_row(&OwnerTimelinePost {
            id: "tl1".into(),
            object_id: "obj1".into(),
            actor_id: "https://example.social/users/alice".into(),
            actor_username: Some("alice".into()),
            actor_display_name: Some("Alice".into()),
            actor_avatar_url: None,
            content: "Unlisted note".into(),
            content_html: None,
            visibility: "unlisted".into(),
            in_reply_to: None,
            published_at: None,
            protocol: Some("activitypub".into()),
            reply_count: 0,
            like_count: 0,
            boost_count: 0,
        });
        assert_eq!(timeline.chip.as_str(), "Unlisted");
        assert!(timeline.meta.contains("Unlisted"));
    }

    #[test]
    fn audience_indicators_distinguish_public_followers_direct_groups_and_e2ee() {
        let public = audience_indicator_for_visibility(&Visibility::Public);
        assert_eq!(public.label, "Public web");
        assert_eq!(public.tone, "warn");

        let followers = audience_indicator_for_visibility(&Visibility::Followers);
        assert_eq!(followers.label, "Followers");
        assert_eq!(followers.tone, "ok");

        let direct_one = audience_indicator_for_target(&Visibility::Direct, false, 1, false);
        assert_eq!(direct_one.label, "1 person");

        let direct_many = audience_indicator_for_target(&Visibility::Direct, false, 3, false);
        assert_eq!(direct_many.label, "3 people");

        let group = audience_indicator_for_target(&Visibility::Direct, false, 0, true);
        assert_eq!(group.label, "Group");

        let encrypted_one = audience_indicator_for_target(&Visibility::Direct, true, 1, false);
        assert_eq!(encrypted_one.label, "E2EE 1:1");

        let encrypted_group = audience_indicator_for_target(&Visibility::Direct, true, 0, true);
        assert_eq!(encrypted_group.label, "E2EE group");
    }

    #[test]
    fn selected_post_inspector_has_first_class_visibility_context() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.inspector_rows("post:fixture-private-post");
        let visibility = rows
            .iter()
            .find(|row| row.id.as_str() == "visibility:post:fixture-private-post")
            .expect("visibility context row");
        assert_eq!(visibility.title.as_str(), "Who can see this");
        assert_eq!(visibility.chip.as_str(), "Followers");
        assert!(visibility.detail.contains("Approved followers"));
    }

    #[test]
    fn notification_context_explains_known_visibility() {
        let row = notification_row(&OwnerNotification {
            id: "n5".into(),
            kind: "reply".into(),
            actor_id: "https://social.example/users/vera".into(),
            actor_username: Some("vera".into()),
            actor_display_name: Some("Vera".into()),
            actor_avatar_url: None,
            post_id: Some("post-1".into()),
            activity_id: None,
            content: Some("Reply text".into()),
            read: serde_json::Value::Bool(false),
            created_at: Some("2m".into()),
            context_post_id: Some("post-1".into()),
            context_post_content: Some("Original context".into()),
            context_post_content_html: None,
            context_post_visibility: Some("followers".into()),
            context_post_protocol: None,
            context_post_published_at: None,
        });
        assert!(row.detail.contains("followers/friends"));
        assert!(row.detail.contains("Reply: Reply text"));
        assert!(row.detail.contains("Original post: Original context"));
    }

    #[test]
    fn reply_notification_inspector_shows_reply_and_original_post() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.inspector_rows("notification:notice-reply");
        let reply = rows
            .iter()
            .find(|row| row.id.as_str() == "notification-reply:notice-reply")
            .expect("reply row");
        assert_eq!(reply.title.as_str(), "Reply text");
        assert!(reply.detail.contains("Can we keep this to close friends?"));
        let original = rows
            .iter()
            .find(|row| row.id.as_str() == "notification-context:notice-reply")
            .expect("original post row");
        assert_eq!(original.title.as_str(), "Original post");
        assert!(original.detail.contains("Original close-friends post"));
    }

    #[test]
    fn notification_inspector_shows_original_context() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.inspector_rows("notification:notice-like-context");
        assert!(rows.iter().any(|row| row.id.as_str()
            == "notification-detail:notice-like-context"
            && row.detail.contains("Someone liked a post")));
        let context = rows
            .iter()
            .find(|row| row.id.as_str() == "notification-context:notice-like-context")
            .expect("notification context row");
        assert_eq!(context.kind.as_str(), "post");
        assert!(context.detail.contains("private post with context"));
        assert!(context.meta.contains("followers/friends"));
        assert_eq!(context.primary.as_str(), "Open context");
        let link = rows
            .iter()
            .find(|row| row.id.as_str().starts_with("url:https://dais.social"))
            .expect("external link row");
        assert_eq!(link.title.as_str(), "External link");
        assert_eq!(link.primary.as_str(), "Open link");
    }

    #[test]
    fn post_inspector_exposes_external_links_without_timeline_clutter() {
        let mut controller = DeskController::fixture_for_tests();
        controller
            .data
            .snapshot
            .home_timeline
            .push(OwnerTimelinePost {
                id: "timeline-link".into(),
                object_id: "https://remote.example/posts/link".into(),
                actor_id: "https://remote.example/users/ada".into(),
                actor_username: Some("@ada@remote.example".into()),
                actor_display_name: Some("Ada".into()),
                actor_avatar_url: None,
                content: "Read the link".into(),
                content_html: Some(
                    "<p>Read <a href=\"https://example.org/article\">the article</a>.</p>".into(),
                ),
                visibility: "public".into(),
                in_reply_to: None,
                published_at: Some("today".into()),
                protocol: Some("ActivityPub".into()),
                reply_count: 0,
                like_count: 0,
                boost_count: 0,
            });
        let timeline = timeline_row(controller.data.snapshot.home_timeline.last().unwrap());
        assert!(!timeline.detail.contains("https://example.org/article"));
        let rows = controller.inspector_rows("timeline:https://remote.example/posts/link");
        assert!(rows
            .iter()
            .any(|row| row.id.as_str() == "url:https://example.org/article"
                && row.primary.as_str() == "Open link"));
    }

    #[test]
    fn thread_reply_rows_show_visibility_when_known() {
        let row = reply_activity_row(
            "post-1",
            0,
            &serde_json::json!({
                "actor_display_name": "Reply Actor",
                "content": "Reply content",
                "published_at": "now",
                "visibility": "public"
            }),
        );
        assert_eq!(row.chip.as_str(), "Public");
        assert_eq!(row.tone.as_str(), "warn");
        assert!(row.detail.contains("anyone"));
    }

    #[test]
    fn extract_first_url_handles_html_and_markdown() {
        assert_eq!(
            extract_first_url("<a href=\"https://social.example/post/1\">read</a>").as_deref(),
            Some("https://social.example/post/1")
        );
        assert_eq!(
            extract_first_url("[read](https://social.example/post/2)").as_deref(),
            Some("https://social.example/post/2")
        );
    }

    #[test]
    fn moderation_reply_row_humanizes_status_and_flags() {
        let row = moderation_reply_row(&ModerationReplyRow {
            id: "m1".into(),
            post_id: "p1".into(),
            actor_id: "https://social.example/users/zeek".into(),
            actor_username: Some("zeek".into()),
            actor_display_name: Some("Zeek".into()),
            actor_avatar_url: None,
            content: "This reply has <script>alert(1)</script> concerns.".into(),
            published_at: Some("now".into()),
            created_at: Some("now".into()),
            moderation_status: Some("needs_review".into()),
            moderation_score: Some(0.5),
            moderation_flags: vec!["violence".into(), "adult".into()],
            moderation_checked_at: None,
            hidden: serde_json::Value::Bool(false),
        });
        assert_eq!(row.subtitle.as_str(), "Needs Review");
        assert_eq!(row.primary.as_str(), "Approve reply");
        assert_eq!(row.secondary.as_str(), "Hide reply");
        assert!(row.detail.starts_with("Flags: violence, adult"));
        assert!(row.detail.contains("Advisory score: 0.50"));
    }

    #[test]
    fn moderation_reply_actions_match_current_state() {
        let base = ModerationReplyRow {
            id: "m-state".into(),
            post_id: "p1".into(),
            actor_id: "https://social.example/users/zeek".into(),
            actor_username: Some("zeek".into()),
            actor_display_name: Some("Zeek".into()),
            actor_avatar_url: None,
            content: "Review me.".into(),
            published_at: Some("now".into()),
            created_at: Some("now".into()),
            moderation_status: Some("approved".into()),
            moderation_score: None,
            moderation_flags: Vec::new(),
            moderation_checked_at: None,
            hidden: serde_json::Value::Bool(false),
        };

        let approved = moderation_reply_row(&base);
        assert_eq!(approved.chip.as_str(), "Approved");
        assert_eq!(approved.primary.as_str(), "Hide reply");
        assert_eq!(approved.secondary.as_str(), "Reject reply");

        let hidden = moderation_reply_row(&ModerationReplyRow {
            moderation_status: Some("hidden".into()),
            hidden: serde_json::Value::Bool(true),
            ..base.clone()
        });
        assert_eq!(hidden.chip.as_str(), "Hidden");
        assert_eq!(hidden.primary.as_str(), "Approve reply");
        assert_eq!(hidden.secondary.as_str(), "Reject reply");

        let rejected = moderation_reply_row(&ModerationReplyRow {
            moderation_status: Some("rejected".into()),
            hidden: serde_json::Value::Bool(true),
            ..base
        });
        assert_eq!(rejected.chip.as_str(), "Rejected");
        assert_eq!(rejected.primary.as_str(), "Approve reply");
        assert_eq!(rejected.secondary.as_str(), "");
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
            "Direct posts require named recipients or an audience group."
        );
    }

    #[test]
    fn compose_allows_direct_audience_group_without_manual_recipients() {
        let compose = ComposeState {
            text: "small group".into(),
            visibility: Visibility::Direct,
            audience_list_id: Some("close-friends".into()),
            ..ComposeState::default()
        };
        assert!(compose_can_send(&compose));
        assert_eq!(compose_warning(&compose), "Ready to send privately.");
    }

    #[test]
    fn local_drafts_persist_and_restore_compose_state() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        controller.drafts_path = temp_dir.path().join("owner-drafts.json");
        controller.compose = ComposeState {
            text: "Sensitive note for later".into(),
            visibility: Visibility::Direct,
            protocol: ProtocolRoute::ActivityPub,
            encrypt: true,
            in_reply_to: Some("post-123".into()),
            audience_list_id: Some("close-friends".into()),
            recipients: "https://friend.example/users/ada".into(),
            media_description: "diagram alt text".into(),
            attachments: vec!["https://social.dais.social/media/_private/token/file.png".into()],
        };

        controller.save_current_draft_inner().expect("save draft");
        let draft_id = controller.drafts.drafts[0].id.clone();
        let loaded = load_drafts_from(&controller.drafts_path).expect("loaded drafts");
        assert_eq!(loaded.drafts.len(), 1);
        assert_eq!(loaded.drafts[0].text, "Sensitive note for later");

        controller.compose = ComposeState::default();
        controller.open_draft(&draft_id).expect("open draft");
        assert_eq!(controller.active_screen, "compose");
        assert_eq!(controller.compose.text, "Sensitive note for later");
        assert_eq!(controller.compose.visibility, Visibility::Direct);
        assert!(controller.compose.encrypt);
        assert_eq!(
            controller.compose.recipients,
            "https://friend.example/users/ada"
        );
        assert_eq!(controller.compose.attachments.len(), 1);
    }

    #[test]
    fn saved_rows_show_only_active_account_drafts() {
        let mut controller = DeskController::fixture_for_tests();
        controller.settings.accounts = vec![
            StoredOwnerAccount {
                id: "account-a".into(),
                label: "Account A".into(),
                instance_url: "https://a.example".into(),
                owner_token: None,
            },
            StoredOwnerAccount {
                id: "account-b".into(),
                label: "Account B".into(),
                instance_url: "https://b.example".into(),
                owner_token: None,
            },
        ];
        controller.settings.active_account_id = Some("account-a".into());
        controller.drafts.drafts = vec![
            StoredDraft {
                id: "draft-a".into(),
                account_id: "account-a".into(),
                text: "A draft".into(),
                visibility: Visibility::Followers,
                protocol: ProtocolRoute::ActivityPub,
                encrypt: false,
                in_reply_to: None,
                audience_list_id: None,
                recipients: String::new(),
                media_description: String::new(),
                attachments: Vec::new(),
                updated_at: "2".into(),
            },
            StoredDraft {
                id: "draft-b".into(),
                account_id: "account-b".into(),
                text: "B draft".into(),
                visibility: Visibility::Public,
                protocol: ProtocolRoute::Both,
                encrypt: false,
                in_reply_to: None,
                audience_list_id: None,
                recipients: String::new(),
                media_description: String::new(),
                attachments: Vec::new(),
                updated_at: "1".into(),
            },
        ];
        controller.select_screen("saved");
        let rows = controller.rows_for_active_screen();
        assert!(rows.iter().any(|row| row.id.as_str() == "draft:draft-a"));
        assert!(!rows.iter().any(|row| row.id.as_str() == "draft:draft-b"));
        assert_supported_row_actions(&rows);
    }

    #[test]
    fn deleting_draft_updates_local_store() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        controller.drafts_path = temp_dir.path().join("owner-drafts.json");
        controller.drafts.drafts = vec![StoredDraft {
            id: "draft-delete".into(),
            account_id: controller.active_account_id(),
            text: "Delete me".into(),
            visibility: Visibility::Followers,
            protocol: ProtocolRoute::ActivityPub,
            encrypt: false,
            in_reply_to: None,
            audience_list_id: None,
            recipients: String::new(),
            media_description: String::new(),
            attachments: Vec::new(),
            updated_at: "1".into(),
        }];

        controller
            .delete_draft("draft-delete")
            .expect("delete draft");
        assert!(controller.drafts.drafts.is_empty());
        let loaded = load_drafts_from(&controller.drafts_path).expect("loaded drafts");
        assert!(loaded.drafts.is_empty());
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

    #[test]
    fn settings_projection_exposes_owner_defaults() {
        let mut controller = DeskController::fixture_for_tests();
        controller.select_screen("settings");
        let projection = controller.projection();
        assert_eq!(projection.settings_default_visibility, "followers");
        assert_eq!(projection.settings_default_protocol, "activitypub");
        assert!(projection.settings_require_authorized_fetch);
        assert!(projection.settings_manually_approves_followers);
    }

    #[test]
    fn profile_save_requires_current_public_preview() {
        let mut controller = DeskController::fixture_for_tests();
        controller.set_profile_form("Person", "Dais Test", "Public summary", "", "");
        assert_eq!(
            controller.save_profile_inner().err(),
            Some(
                "preview the public identity first; changed fields require a fresh preview".into()
            )
        );

        controller.preview_profile_from_form("Person", "Dais Test", "Public summary", "", "");
        assert_eq!(
            controller.save_profile_inner().as_deref(),
            Ok("Preview profile saved. Add an owner token to update the server.")
        );

        controller.set_profile_form("Person", "Changed", "Public summary", "", "");
        assert_eq!(
            controller.save_profile_inner().err(),
            Some(
                "preview the public identity first; changed fields require a fresh preview".into()
            )
        );
    }

    #[test]
    fn fixture_people_mode_has_expected_screen_order() {
        let mut controller = DeskController::fixture_for_tests();
        controller.select_mode("people");
        let projection = controller.projection();
        let screens: Vec<_> = projection
            .screen_nav
            .iter()
            .map(|item| item.id.to_string())
            .collect();
        assert_eq!(
            screens,
            vec![
                "find",
                "relationship",
                "friends",
                "followers",
                "following",
                "watches",
                "audience",
                "blocks",
            ]
        );
    }

    #[test]
    fn home_today_rows_are_attention_first() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.home_today_rows();
        assert_eq!(rows[0].id.as_str(), "notification:notice-like-context");
        assert_eq!(rows[1].id.as_str(), "notification:notice-reply");
        assert_eq!(rows[2].id.as_str(), "dm:dm-fixture");
        assert_eq!(rows[3].id.as_str(), "timeline:fixture-private-post");
        assert_eq!(rows[4].id.as_str(), "post:fixture-private-post");
    }

    #[test]
    fn reading_rows_include_followed_watched_and_source_posts() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.reading_rows();
        assert!(rows
            .iter()
            .any(|row| row.id.as_str() == "timeline:fixture-private-post"
                && row.subtitle.contains("Following")));
        assert!(rows
            .iter()
            .any(|row| row.title.as_str() == "Nobel Prize public update"
                && row.subtitle.as_str() == "Watched public post"
                && row.chip.as_str() == "Watch"));
        assert!(rows
            .iter()
            .any(|row| row.title.as_str() == "Science source item"
                && row.subtitle.as_str() == "Source post"
                && row.chip.as_str() == "Source"));
    }

    #[test]
    fn inbox_rows_are_attention_first() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.inbox_rows();
        assert_eq!(rows[0].id.as_str(), "notification:notice-like-context");
        assert_eq!(rows[1].id.as_str(), "notification:notice-reply");
        assert_eq!(rows[2].id.as_str(), "dm:dm-fixture");
        assert_eq!(
            rows[3].id.as_str(),
            "follower:https://new.example/users/follower"
        );
        assert_eq!(rows[4].id.as_str(), "moderation-reply:mod-reply-sensitive");
        assert_eq!(rows[5].id.as_str(), "delivery:delivery-failed");
    }

    #[test]
    fn health_rows_show_operator_summary_before_raw_diagnostics() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.health_rows();
        assert_eq!(rows[0].id.as_str(), "health:owner-api");
        assert_eq!(rows[1].id.as_str(), "health:privacy");
        assert_eq!(rows[2].id.as_str(), "health:queues");
        assert_eq!(rows[3].id.as_str(), "health:graph");
        assert_eq!(rows[4].id.as_str(), "health:profile");
        assert!(rows.iter().any(|row| row.id.starts_with("diagnostic:")));
        assert_eq!(rows[0].primary.as_str(), "Refresh");
        assert_eq!(rows[0].secondary.as_str(), "Copy evidence");
        assert_eq!(rows[2].primary.as_str(), "Inspect delivery");
    }

    #[test]
    fn health_delivery_action_opens_failed_delivery() {
        let mut controller = DeskController::fixture_for_tests();
        controller.row_action("health:queues", "Inspect delivery");
        assert_eq!(controller.active_mode, "server");
        assert_eq!(controller.active_screen, "deliveries");
        assert_eq!(controller.selected_row, "delivery:delivery-failed");
    }

    #[test]
    fn delivery_rows_only_show_status_appropriate_actions() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.delivery_rows();
        let failed = rows
            .iter()
            .find(|row| row.id.as_str() == "delivery:delivery-failed")
            .expect("failed delivery row");
        assert_eq!(failed.primary.as_str(), "Retry delivery");
        assert_eq!(failed.secondary.as_str(), "Cancel delivery");

        let delivered = rows
            .iter()
            .find(|row| row.id.as_str() == "delivery:delivery-ok")
            .expect("delivered delivery row");
        assert_eq!(delivered.primary.as_str(), "Open context");
        assert_eq!(delivered.secondary.as_str(), "");
    }

    #[test]
    fn delivery_inspector_explains_failure_and_target() {
        let controller = DeskController::fixture_for_tests();
        let rows = controller.inspector_rows("delivery:delivery-failed");
        assert_supported_row_actions(&rows);
        assert!(rows
            .iter()
            .any(|row| row.id.as_str() == "delivery-detail:delivery-failed"));
        let target = rows
            .iter()
            .find(|row| row.title.as_str() == "Remote target")
            .expect("target row");
        assert_eq!(target.primary.as_str(), "Open link");
        assert_eq!(target.secondary.as_str(), "Copy evidence");
        let failure = rows
            .iter()
            .find(|row| row.id.as_str() == "delivery-failure:delivery-failed")
            .expect("failure row");
        assert_eq!(failure.primary.as_str(), "Retry delivery");
        assert_eq!(failure.secondary.as_str(), "Cancel delivery");
        assert!(failure
            .detail
            .contains("Raw error: Remote host returned 502"));
    }

    #[test]
    fn delivery_rows_resolve_external_target_url() {
        let controller = DeskController::fixture_for_tests();
        assert_eq!(
            resolve_external_url(&controller, "delivery:delivery-failed").as_deref(),
            Ok("https://remote.example/inbox")
        );
        assert_eq!(
            resolve_external_url(&controller, "delivery-detail:delivery-failed").as_deref(),
            Ok("https://remote.example/inbox")
        );
    }

    #[test]
    fn discovered_actor_isolated_to_find_screen() {
        let mut controller = DeskController::fixture_for_tests();
        controller.data.discovered_actor = Some(OwnerDiscoveredActor {
            id: "https://discover.example/users/agent".into(),
            actor_type: None,
            inbox: "https://discover.example/inbox".into(),
            shared_inbox: None,
            preferred_username: Some("agent".into()),
            name: Some("Discovered Agent".into()),
            summary: Some("Public demo discovery row.".into()),
            url: None,
            icon_url: None,
            handle: Some("@agent@discover.example".into()),
            following_status: None,
            target_public_post: None,
            recent_public_posts: Vec::new(),
        });
        controller.select_screen("find");
        let find_rows = controller.rows_for_active_screen();
        assert!(find_rows
            .iter()
            .any(|row| row.id.as_str() == "actor:https://discover.example/users/agent"));

        controller.select_screen("relationship");
        let relationship_rows = controller.rows_for_active_screen();
        assert!(!relationship_rows
            .iter()
            .any(|row| row.id.as_str() == "actor:https://discover.example/users/agent"));
    }

    #[test]
    fn inspector_exposes_secondary_actions_when_list_suppresses_them() {
        let mut controller = DeskController::fixture_for_tests();
        controller.run_command("science");
        controller.select_row("actor:https://mastodon.example/users/science");
        let list_rows = controller.rows_for_active_screen();
        let list_row = list_rows
            .iter()
            .find(|row| row.id.as_str() == "actor:https://mastodon.example/users/science")
            .expect("follow target in list");
        assert_eq!(list_row.secondary.as_str(), "");

        let inspector_rows =
            controller.inspector_rows("actor:https://mastodon.example/users/science");
        let inspector_row = inspector_rows
            .into_iter()
            .find(|row| row.id.as_str() == "actor:https://mastodon.example/users/science")
            .expect("selected row in inspector");
        assert_eq!(inspector_row.primary.as_str(), "Follow");
        assert_eq!(inspector_row.secondary.as_str(), "Watch");
    }

    #[test]
    fn account_rows_hide_delete_when_only_one_account_exists() {
        let mut controller = DeskController::fixture_for_tests();
        controller.select_screen("accounts");
        let rows = controller.rows_for_active_screen();
        assert!(rows.iter().all(|row| row.secondary.as_str() != "Delete"));
        let projection = controller.projection();
        assert!(projection
            .accounts
            .iter()
            .all(|account| !account.can_delete));
    }

    #[test]
    fn account_rows_show_expected_actions_for_active_and_inactive_accounts() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let settings_path = temp_dir.path().join("owner-settings.json");
        controller.settings_path = settings_path;
        controller.settings.accounts = vec![
            StoredOwnerAccount {
                id: "account-a".into(),
                label: "Account A".into(),
                instance_url: "https://a.example".into(),
                owner_token: Some("token-a".into()),
            },
            StoredOwnerAccount {
                id: "account-b".into(),
                label: "Account B".into(),
                instance_url: "https://b.example".into(),
                owner_token: None,
            },
        ];
        controller.settings.active_account_id = Some("account-a".into());
        controller.settings.instance_url = "https://a.example".into();
        controller.settings.owner_token = Some("token-a".into());
        controller.select_screen("accounts");
        let rows = controller.rows_for_active_screen();
        assert_eq!(rows.len(), 2);
        assert_supported_row_actions(&rows);
        let (active, inactive) = (rows[0].clone(), rows[1].clone());
        assert_eq!(active.primary.as_str(), "Validate token");
        assert_eq!(active.chip.as_str(), "Active");
        assert_eq!(inactive.primary.as_str(), "Switch");
        assert_eq!(inactive.secondary.as_str(), "Delete");
        assert_eq!(inactive.chip.as_str(), "Account");
    }

    #[test]
    fn switching_account_forces_active_id() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        controller.settings_path = temp_dir.path().join("owner-settings.json");
        controller.settings.accounts = vec![
            StoredOwnerAccount {
                id: "account-a".into(),
                label: "Account A".into(),
                instance_url: "https://a.example".into(),
                owner_token: Some("token-a".into()),
            },
            StoredOwnerAccount {
                id: "account-b".into(),
                label: "Account B".into(),
                instance_url: "https://b.example".into(),
                owner_token: None,
            },
        ];
        controller.settings.active_account_id = Some("account-a".into());
        assert_eq!(
            controller
                .switch_account_result("account-b")
                .expect("switch"),
            "Switched account. Reads, posts, follows, watches, and server commands use it now."
        );
        assert_eq!(
            controller.settings.active_account_id,
            Some("account-b".to_string())
        );
    }

    #[test]
    fn validating_account_token_requires_stored_token() {
        let mut controller = DeskController::fixture_for_tests();
        controller.settings.accounts = vec![StoredOwnerAccount {
            id: "account-a".into(),
            label: "Account A".into(),
            instance_url: "https://a.example".into(),
            owner_token: None,
        }];
        assert_eq!(
            controller.validate_account_token("account-a").err(),
            Some("owner token is required for validation".into())
        );
        assert_eq!(
            controller.validate_account_token("missing").err(),
            Some("account not found".into())
        );
    }

    #[test]
    fn deleting_account_moves_active_to_first_remaining_and_blocks_last_account_deletion() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        controller.settings_path = temp_dir.path().join("owner-settings.json");
        controller.settings.accounts = vec![
            StoredOwnerAccount {
                id: "account-a".into(),
                label: "Account A".into(),
                instance_url: "https://a.example".into(),
                owner_token: Some("token-a".into()),
            },
            StoredOwnerAccount {
                id: "account-b".into(),
                label: "Account B".into(),
                instance_url: "https://b.example".into(),
                owner_token: None,
            },
        ];
        controller.settings.active_account_id = Some("account-b".into());
        assert_eq!(
            controller
                .delete_account_result("account-b")
                .expect("delete"),
            "Deleted account profile."
        );
        assert_eq!(controller.settings.accounts.len(), 1);
        assert_eq!(
            controller.settings.active_account_id,
            Some("account-a".to_string())
        );
        assert_eq!(
            controller.delete_account_result("account-a").err(),
            Some("at least one account profile is required".into())
        );
    }

    #[test]
    fn row_action_supports_switch_and_delete_for_account_rows() {
        let mut controller = DeskController::fixture_for_tests();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        controller.settings_path = temp_dir.path().join("owner-settings.json");
        controller.settings.accounts = vec![
            StoredOwnerAccount {
                id: "account-a".into(),
                label: "Account A".into(),
                instance_url: "https://a.example".into(),
                owner_token: Some("token-a".into()),
            },
            StoredOwnerAccount {
                id: "account-b".into(),
                label: "Account B".into(),
                instance_url: "https://b.example".into(),
                owner_token: None,
            },
        ];
        controller.settings.active_account_id = Some("account-a".into());
        controller.row_action("account:account-b", "Switch");
        assert_eq!(
            controller.settings.active_account_id,
            Some("account-b".into())
        );

        controller.row_action("account:account-b", "Delete");
        assert_eq!(controller.settings.accounts.len(), 1);
        assert_eq!(
            controller.settings.active_account_id,
            Some("account-a".into())
        );
    }

    #[test]
    fn selecting_audience_screen_prefills_editor() {
        let mut controller = DeskController::fixture_for_tests();
        controller.select_screen("audience");
        let projection = controller.projection();
        assert_eq!(projection.audience_id, "close-friends");
        assert_eq!(projection.audience_name, "Close Friends");
        assert!(projection.audience_members.contains("friend.example"));
    }

    #[test]
    fn audience_group_row_can_target_compose() {
        let mut controller = DeskController::fixture_for_tests();
        controller.row_action("audience:close-friends", "Use in compose");
        assert_eq!(controller.active_screen, "compose");
        assert_eq!(controller.compose.visibility, Visibility::Direct);
        assert_eq!(
            controller.compose.audience_list_id.as_deref(),
            Some("close-friends")
        );
        let projection = controller.projection();
        assert!(projection
            .compose_audience_summary
            .contains("Close Friends"));
        assert!(projection.compose_audience_summary.contains("1 member"));
    }

    #[test]
    fn compose_context_rows_explain_visibility_and_reply_context() {
        let mut controller = DeskController::fixture_for_tests();
        controller.row_action("notification:notice-reply", "Reply");
        let rows = controller.compose_context_rows();
        assert!(rows
            .iter()
            .any(|row| row.id.as_str() == "compose:visibility-summary"
                && row.detail.contains("approved followers")));
        assert!(rows.iter().any(|row| row.title.as_str() == "Reply context"
            && !row.detail.contains("fixture-private-post")));
    }

    #[test]
    fn selecting_post_loads_thread_detail_for_inspector() {
        let mut controller = DeskController::fixture_for_tests();
        controller.select_row("post:fixture-private-post");
        let projection = controller.projection();
        assert!(projection
            .inspector_rows
            .iter()
            .any(|row| row.title.as_str() == "Thread detail"));
        assert!(projection
            .inspector_rows
            .iter()
            .any(|row| row.id.as_str().contains(":reply:")));
    }

    #[test]
    fn replying_to_notification_preserves_post_context() {
        let mut controller = DeskController::fixture_for_tests();
        controller.row_action("notification:notice-reply", "Reply");
        assert_eq!(controller.active_screen, "compose");
        assert_eq!(
            controller.compose.in_reply_to.as_deref(),
            Some("fixture-private-post")
        );
        assert_eq!(controller.compose.visibility, Visibility::Followers);
    }

    #[test]
    fn replying_to_dm_sets_direct_recipient() {
        let mut controller = DeskController::fixture_for_tests();
        controller.row_action("dm:dm-fixture", "Reply");
        assert_eq!(controller.active_screen, "compose");
        assert_eq!(controller.compose.visibility, Visibility::Direct);
        assert_eq!(
            controller.compose.recipients,
            "https://friend.example/users/ada"
        );
    }

    #[test]
    fn infers_protocol_specific_watch_types() {
        assert_eq!(
            infer_watch_type("https://bsky.app/profile/nasa.gov/post/abc"),
            "bluesky_post"
        );
        assert_eq!(infer_watch_type("nasa.gov"), "bluesky_actor");
        assert_eq!(
            infer_watch_type("https://example.social/users/alice/statuses/1"),
            "activitypub_object"
        );
        assert_eq!(infer_watch_type("https://example.com/feed.xml"), "rss");
    }

    #[test]
    fn selecting_media_file_infers_type_without_overwriting_manual_type() {
        let mut controller = DeskController::fixture_for_tests();
        controller.set_media_file_path("/tmp/photo.png");
        assert_eq!(controller.media_form.file_path, "/tmp/photo.png");
        assert_eq!(controller.media_form.media_type, "image/png");

        controller.media_form.media_type = "image/custom".into();
        controller.set_media_file_path("/tmp/movie.mp4");
        assert_eq!(controller.media_form.media_type, "image/custom");
    }
}
