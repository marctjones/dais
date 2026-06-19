use dais_client_core::{
    ComposeDraft, DiagnosticStatus, ModerationReplyRow, ModerationSettingsUpdate, ModerationState,
    OwnerActionResult, OwnerApiClient, OwnerAudienceList, OwnerAudienceListUpsert,
    OwnerCreatedPost, OwnerDeletedPost, OwnerDelivery, OwnerDiscoveredActor, OwnerFollowResult,
    OwnerInteraction, OwnerInteractionResult, OwnerMedia, OwnerMediaUpload, OwnerNotification,
    OwnerPost, OwnerPostDetail, OwnerProfile, OwnerProfileUpdate, OwnerSearchQuery,
    OwnerSearchResult, OwnerSection, OwnerSettings, OwnerSnapshot, OwnerSourceAdd,
    OwnerSourceAddResult, OwnerSourceRefreshResult, OwnerSources, OwnerStats, OwnerWatchAdd,
    ProtocolRoute, SourceItem, Visibility,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredOwnerSettings {
    #[serde(default = "default_instance_url")]
    instance_url: String,
    #[serde(default)]
    owner_token: Option<String>,
    #[serde(default)]
    active_account_id: Option<String>,
    #[serde(default)]
    accounts: Vec<StoredOwnerAccount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredOwnerAccount {
    id: String,
    label: String,
    instance_url: String,
    owner_token: Option<String>,
}

impl Default for StoredOwnerSettings {
    fn default() -> Self {
        let account = StoredOwnerAccount {
            id: account_id_for(&default_instance_url(), &[]),
            label: "Dais Social".to_string(),
            instance_url: default_instance_url(),
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

#[derive(Clone, Debug, Serialize)]
struct OwnerAccountSummary {
    id: String,
    label: String,
    instance_url: String,
    active: bool,
    owner_token_present: bool,
}

fn default_instance_url() -> String {
    "https://social.dais.social".to_string()
}

#[tauri::command]
async fn owner_snapshot(app: tauri::AppHandle) -> Result<OwnerSnapshot, String> {
    let stored = load_settings(&app)?;
    if let Some(token) = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let client = OwnerApiClient::new(&stored.instance_url, token);
        match client.snapshot().await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => return Ok(local_snapshot(stored, Some(error.to_string()))),
        }
    }
    Ok(local_snapshot(stored, None))
}

#[tauri::command]
async fn create_owner_post(
    app: tauri::AppHandle,
    text: String,
    visibility: Visibility,
    protocol: ProtocolRoute,
    encrypt: bool,
    in_reply_to: Option<String>,
    audience_list_id: Option<String>,
    recipients: Vec<String>,
    attachments: Vec<String>,
) -> Result<OwnerCreatedPost, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .create_post(&ComposeDraft {
            text,
            visibility,
            protocol,
            encrypt,
            in_reply_to: optional_trimmed(in_reply_to.unwrap_or_default()),
            audience_list_id: optional_trimmed(audience_list_id.unwrap_or_default()),
            recipients,
            attachments,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_owner_post(
    app: tauri::AppHandle,
    object_id: String,
) -> Result<OwnerDeletedPost, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .delete_post(&object_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_interaction(
    app: tauri::AppHandle,
    object_id: String,
    interaction: String,
) -> Result<OwnerInteractionResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .interact(&OwnerInteraction {
            object_id,
            interaction,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_post_detail(
    app: tauri::AppHandle,
    object_id: String,
) -> Result<OwnerPostDetail, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .post_detail(&object_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_notifications(app: tauri::AppHandle) -> Result<Vec<OwnerNotification>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .notifications()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn mark_owner_notification_read(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .mark_notification_read(&id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_deliveries(app: tauri::AppHandle) -> Result<Vec<OwnerDelivery>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client.deliveries().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_direct_messages(
    app: tauri::AppHandle,
) -> Result<Vec<dais_client_core::OwnerDirectMessage>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .direct_messages()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_search(
    app: tauri::AppHandle,
    query: String,
    scope: Option<String>,
    provider: Option<String>,
    result_type: Option<String>,
    servers: Option<Vec<String>>,
    sort: Option<String>,
    since: Option<String>,
    until: Option<String>,
    author: Option<String>,
    mentions: Option<String>,
    lang: Option<String>,
    domain: Option<String>,
    url: Option<String>,
    tags: Option<Vec<String>>,
    confirm_public_sensitive: Option<bool>,
) -> Result<OwnerSearchResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .search_with_options(&OwnerSearchQuery {
            query,
            scope: scope.unwrap_or_else(|| "local".to_string()),
            confirm_public_sensitive: confirm_public_sensitive.unwrap_or(false),
            provider,
            result_type,
            servers: servers.unwrap_or_default(),
            sort,
            since,
            until,
            author,
            mentions,
            lang,
            domain,
            url,
            tags: tags.unwrap_or_default(),
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_stats(app: tauri::AppHandle) -> Result<OwnerStats, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client.stats().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_diagnostics(app: tauri::AppHandle) -> Result<Vec<DiagnosticStatus>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .diagnostics()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_sources(app: tauri::AppHandle) -> Result<OwnerSources, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client.sources().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_watches(app: tauri::AppHandle) -> Result<OwnerSources, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client.watches().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_owner_source(
    app: tauri::AppHandle,
    source_type: String,
    url: String,
    title: Option<String>,
    cadence_minutes: Option<u16>,
) -> Result<OwnerSourceAddResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .add_source(&OwnerSourceAdd {
            source_type,
            url,
            title,
            cadence_minutes,
            api_secret_name: None,
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            image_allowed: false,
            full_text_allowed: false,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_owner_watch(
    app: tauri::AppHandle,
    watch_type: String,
    target: String,
    title: Option<String>,
    cadence_minutes: Option<u16>,
) -> Result<OwnerSourceAddResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .add_watch(&OwnerWatchAdd {
            watch_type,
            target,
            title,
            cadence_minutes,
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            image_allowed: false,
            full_text_allowed: false,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_owner_source(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .remove_source(&id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_owner_watch(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .remove_watch(&id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_owner_source(
    app: tauri::AppHandle,
    id: Option<String>,
) -> Result<OwnerSourceRefreshResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .refresh_sources(id.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refresh_owner_watch(
    app: tauri::AppHandle,
    id: Option<String>,
) -> Result<OwnerSourceRefreshResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .refresh_watches(id.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_moderation(app: tauri::AppHandle) -> Result<ModerationState, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client.moderation().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_moderation_replies(
    app: tauri::AppHandle,
) -> Result<Vec<ModerationReplyRow>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .moderation_replies()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_owner_reply_moderation_status(
    app: tauri::AppHandle,
    reply_id: String,
    status: String,
) -> Result<ModerationReplyRow, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .set_reply_moderation_status(&reply_id, &status)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_owner_moderation_settings(
    app: tauri::AppHandle,
    reply_policy: String,
    ai_enabled: bool,
    ai_model: Option<String>,
    ai_daily_budget: u64,
) -> Result<ModerationState, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .update_moderation_settings(&ModerationSettingsUpdate {
            reply_policy,
            ai_enabled,
            ai_model: ai_model.and_then(optional_trimmed),
            ai_daily_budget,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn block_owner_actor(
    app: tauri::AppHandle,
    actor_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .block_actor(&actor_id, reason.as_deref())
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn block_owner_domain(
    app: tauri::AppHandle,
    domain: String,
    reason: Option<String>,
) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .block_domain(&domain, reason.as_deref())
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn unblock_owner_value(app: tauri::AppHandle, value: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .unblock(&value)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn allow_owner_host(
    app: tauri::AppHandle,
    host: String,
    note: Option<String>,
) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .allow_host(&host, note.as_deref())
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn disallow_owner_host(app: tauri::AppHandle, host: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .disallow_host(&host)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn discover_actor(
    app: tauri::AppHandle,
    target: String,
) -> Result<OwnerDiscoveredActor, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .discover_actor(&target)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn upload_owner_media(
    app: tauri::AppHandle,
    filename: String,
    media_type: Option<String>,
    access: Option<String>,
    data_base64: String,
) -> Result<OwnerMedia, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .upload_media(&OwnerMediaUpload {
            filename,
            media_type,
            access,
            expires_in_seconds: None,
            require_authorized_fetch: None,
            data_base64,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn revoke_owner_media(
    app: tauri::AppHandle,
    url: String,
) -> Result<OwnerActionResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .revoke_media(&url)
        .await
        .map_err(|error| error.to_string())
}

fn local_snapshot(stored: StoredOwnerSettings, api_error: Option<String>) -> OwnerSnapshot {
    let instance_url = stored.instance_url;
    let owner_token_present = stored
        .owner_token
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let owner_api_ok = api_error.is_none() && owner_token_present;
    OwnerSnapshot {
        settings: OwnerSettings {
            instance_url,
            owner_token_present,
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
        posts: vec![OwnerPost {
            id: "draft-local-preview".to_string(),
            title: Some("Owner app shell".to_string()),
            content: "Private-by-default compose, sources, moderation, delivery, diagnostics, and profile screens are scaffolded for the owner API.".to_string(),
            visibility: Visibility::Followers,
            protocol: ProtocolRoute::ActivityPub,
            encrypted: false,
            attachments: Vec::new(),
            reply_count: 0,
            like_count: 0,
            boost_count: 0,
            published_at: None,
        }],
        followers: Vec::new(),
        friends: Vec::new(),
        following: Vec::new(),
        audience_lists: Vec::new(),
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
            block_count: 0,
            allowlist_count: 0,
            require_authorized_fetch: true,
            manually_approves_followers: true,
            reply_policy: "warn".to_string(),
            ai_enabled: false,
            ai_model: Some("@cf/meta/llama-guard-3-8b".to_string()),
            ai_daily_budget: 0,
            reply_queue_count: 0,
            flagged_reply_count: 0,
            hidden_reply_count: 0,
            rejected_reply_count: 0,
            blocks: Vec::new(),
            allowlist: Vec::new(),
        },
        diagnostics: vec![
            DiagnosticStatus {
                key: "owner-api".to_string(),
                ok: owner_api_ok,
                detail: api_error
                    .unwrap_or_else(|| "No owner API token stored; showing local preview data.".to_string()),
            },
            DiagnosticStatus {
                key: "adaptive-layout".to_string(),
                ok: true,
                detail: "Navigation collapses for narrow/mobile widths.".to_string(),
            },
        ],
    }
}

#[tauri::command]
fn save_owner_settings(
    app: tauri::AppHandle,
    instance_url: String,
    owner_token: String,
    account_id: Option<String>,
    label: Option<String>,
) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    let instance_url =
        normalize_instance_url(&instance_url).unwrap_or_else(|| settings.instance_url.clone());
    let label = label
        .and_then(optional_trimmed)
        .unwrap_or_else(|| account_label(&instance_url));
    let account_id = account_id.and_then(optional_trimmed);
    let existing_index = account_id
        .as_deref()
        .and_then(|id| {
            settings
                .accounts
                .iter()
                .position(|account| account.id == id)
        })
        .or_else(|| {
            settings
                .accounts
                .iter()
                .position(|account| account.instance_url == instance_url)
        });
    let saved_id = if let Some(index) = existing_index {
        let account = &mut settings.accounts[index];
        account.label = label;
        account.instance_url = instance_url;
        if !owner_token.is_empty() {
            account.owner_token = Some(owner_token);
        }
        account.id.clone()
    } else {
        let existing_ids: Vec<String> = settings
            .accounts
            .iter()
            .map(|account| account.id.clone())
            .collect();
        let account = StoredOwnerAccount {
            id: account_id.unwrap_or_else(|| account_id_for(&instance_url, &existing_ids)),
            label,
            instance_url,
            owner_token: (!owner_token.is_empty()).then_some(owner_token),
        };
        let saved_id = account.id.clone();
        settings.accounts.push(account);
        saved_id
    };
    settings.active_account_id = Some(saved_id);
    persist_settings(&app, normalize_settings(settings))
}

#[tauri::command]
fn owner_accounts(app: tauri::AppHandle) -> Result<Vec<OwnerAccountSummary>, String> {
    let settings = load_settings(&app)?;
    Ok(account_summaries(&settings))
}

#[tauri::command]
fn switch_owner_account(app: tauri::AppHandle, account_id: String) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    if !settings
        .accounts
        .iter()
        .any(|account| account.id == account_id)
    {
        return Err("account not found".to_string());
    }
    settings.active_account_id = Some(account_id);
    persist_settings(&app, normalize_settings(settings))
}

#[tauri::command]
fn delete_owner_account(app: tauri::AppHandle, account_id: String) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    if settings.accounts.len() <= 1 {
        return Err("at least one account is required".to_string());
    }
    let before = settings.accounts.len();
    settings.accounts.retain(|account| account.id != account_id);
    if settings.accounts.len() == before {
        return Err("account not found".to_string());
    }
    if settings.active_account_id.as_deref() == Some(account_id.as_str()) {
        settings.active_account_id = settings.accounts.first().map(|account| account.id.clone());
    }
    persist_settings(&app, normalize_settings(settings))
}

#[tauri::command]
async fn update_owner_profile(
    app: tauri::AppHandle,
    actor_type: String,
    display_name: String,
    summary: String,
    icon: String,
    image: String,
) -> Result<OwnerProfile, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .update_profile(&OwnerProfileUpdate {
            actor_type: optional_trimmed(actor_type),
            display_name: optional_trimmed(display_name),
            summary: optional_trimmed(summary),
            icon: optional_trimmed(icon),
            image: optional_trimmed(image),
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn follow_actor(app: tauri::AppHandle, target: String) -> Result<OwnerFollowResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .follow_actor(&target)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn unfollow_actor(
    app: tauri::AppHandle,
    target: String,
) -> Result<OwnerFollowResult, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .unfollow_actor(&target)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_follower_status(
    app: tauri::AppHandle,
    follower_actor_id: String,
    status: String,
) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .set_follower_status(&follower_actor_id, &status)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn owner_audience_lists(app: tauri::AppHandle) -> Result<Vec<OwnerAudienceList>, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .audience_lists()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn upsert_owner_audience_list(
    app: tauri::AppHandle,
    id: Option<String>,
    name: String,
    description: Option<String>,
    allowed_categories: Vec<String>,
    member_actor_ids: Vec<String>,
) -> Result<OwnerAudienceList, String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .upsert_audience_list(&OwnerAudienceListUpsert {
            id: id.and_then(optional_trimmed),
            name,
            description: description.and_then(optional_trimmed),
            allowed_categories,
            member_actor_ids,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_owner_audience_list(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let stored = load_settings(&app)?;
    let token = stored
        .owner_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "owner token is required".to_string())?;
    let client = OwnerApiClient::new(&stored.instance_url, token);
    client
        .delete_audience_list(&id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn optional_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn load_settings(app: &tauri::AppHandle) -> Result<StoredOwnerSettings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(StoredOwnerSettings::default());
    }
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let settings: StoredOwnerSettings =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    Ok(normalize_settings(settings))
}

fn persist_settings(app: &tauri::AppHandle, settings: StoredOwnerSettings) -> Result<(), String> {
    let path = settings_path(app)?;
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
        if account.label.trim().is_empty() {
            account.label = account_label(&account.instance_url);
        } else {
            account.label = account.label.trim().to_string();
        }
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

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    Ok(base.join("owner-settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_single_account_settings() {
        let settings = normalize_settings(StoredOwnerSettings {
            instance_url: "joneslaw.io/".to_string(),
            owner_token: Some("owner-token".to_string()),
            active_account_id: None,
            accounts: Vec::new(),
        });

        assert_eq!(settings.instance_url, "https://joneslaw.io");
        assert_eq!(settings.owner_token.as_deref(), Some("owner-token"));
        assert_eq!(settings.accounts.len(), 1);
        assert_eq!(settings.accounts[0].label, "joneslaw.io");
        assert_eq!(
            settings.active_account_id.as_deref(),
            Some("account-joneslaw-io")
        );
    }

    #[test]
    fn mirrors_active_account_to_legacy_fields() {
        let settings = normalize_settings(StoredOwnerSettings {
            instance_url: "https://social.dais.social".to_string(),
            owner_token: Some("old-token".to_string()),
            active_account_id: Some("account-skeptical-engineer".to_string()),
            accounts: vec![
                StoredOwnerAccount {
                    id: "account-dais-social".to_string(),
                    label: "Dais Social".to_string(),
                    instance_url: "https://social.dais.social".to_string(),
                    owner_token: Some("dais-token".to_string()),
                },
                StoredOwnerAccount {
                    id: "account-skeptical-engineer".to_string(),
                    label: "Skeptical Engineer".to_string(),
                    instance_url: "skeptical.engineer".to_string(),
                    owner_token: Some("skeptical-token".to_string()),
                },
            ],
        });

        assert_eq!(settings.instance_url, "https://skeptical.engineer");
        assert_eq!(settings.owner_token.as_deref(), Some("skeptical-token"));
        assert_eq!(
            account_summaries(&settings)
                .into_iter()
                .find(|account| account.active)
                .map(|account| account.label),
            Some("Skeptical Engineer".to_string())
        );
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            owner_snapshot,
            save_owner_settings,
            owner_accounts,
            switch_owner_account,
            delete_owner_account,
            create_owner_post,
            delete_owner_post,
            upload_owner_media,
            revoke_owner_media,
            owner_interaction,
            owner_post_detail,
            owner_notifications,
            mark_owner_notification_read,
            owner_deliveries,
            owner_direct_messages,
            owner_search,
            owner_stats,
            owner_diagnostics,
            owner_audience_lists,
            upsert_owner_audience_list,
            delete_owner_audience_list,
            owner_sources,
            owner_watches,
            add_owner_source,
            add_owner_watch,
            remove_owner_source,
            remove_owner_watch,
            refresh_owner_source,
            refresh_owner_watch,
            owner_moderation,
            owner_moderation_replies,
            set_owner_reply_moderation_status,
            save_owner_moderation_settings,
            block_owner_actor,
            block_owner_domain,
            unblock_owner_value,
            allow_owner_host,
            disallow_owner_host,
            discover_actor,
            update_owner_profile,
            follow_actor,
            unfollow_actor,
            set_follower_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running dais owner app");
}
