use dais_client_core::{
    ComposeDraft, DiagnosticStatus, ModerationReplyRow, ModerationSettingsUpdate, ModerationState,
    OwnerApiClient, OwnerAudienceList, OwnerAudienceListUpsert, OwnerCreatedPost, OwnerDelivery,
    OwnerDiscoveredActor, OwnerFollowResult, OwnerInteraction, OwnerInteractionResult, OwnerMedia,
    OwnerMediaUpload, OwnerNotification, OwnerPost, OwnerPostDetail, OwnerProfile,
    OwnerProfileUpdate, OwnerSearchResult, OwnerSection, OwnerSettings, OwnerSnapshot,
    OwnerSourceAdd, OwnerSourceAddResult, OwnerSourceRefreshResult, OwnerSources, OwnerStats,
    OwnerWatchAdd, ProtocolRoute, SourceItem, Visibility,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredOwnerSettings {
    instance_url: String,
    owner_token: Option<String>,
}

impl Default for StoredOwnerSettings {
    fn default() -> Self {
        Self {
            instance_url: "https://social.dais.social".to_string(),
            owner_token: None,
        }
    }
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
        .search_with_scope_confirmation(
            &query,
            scope.as_deref().unwrap_or("local"),
            confirm_public_sensitive.unwrap_or(false),
        )
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
) -> Result<(), String> {
    let mut settings = load_settings(&app)?;
    if !instance_url.trim().is_empty() {
        settings.instance_url = instance_url.trim().trim_end_matches('/').to_string();
    }
    if !owner_token.is_empty() {
        settings.owner_token = Some(owner_token);
    }
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
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
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    Ok(base.join("owner-settings.json"))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            owner_snapshot,
            save_owner_settings,
            create_owner_post,
            upload_owner_media,
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
