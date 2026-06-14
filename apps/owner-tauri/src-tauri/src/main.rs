use dais_client_core::{
    ComposeDraft, DiagnosticStatus, ModerationState, OwnerApiClient, OwnerCreatedPost,
    OwnerFollowResult, OwnerInteraction, OwnerInteractionResult, OwnerMedia, OwnerMediaUpload,
    OwnerPost, OwnerProfile, OwnerProfileUpdate, OwnerSection, OwnerSettings, OwnerSnapshot,
    ProtocolRoute, SourceItem, Visibility,
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
        following: Vec::new(),
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
            update_owner_profile,
            follow_actor,
            unfollow_actor,
            set_follower_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running dais owner app");
}
