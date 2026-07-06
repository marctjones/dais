use crate::public_https_url;
use crate::request::string_like_any;
use serde_json::Value;
use sha2::{Digest, Sha256};

const SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "activitypub",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const REFRESHABLE_SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const WATCH_SOURCE_TYPES: &[&str] = &[
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];

pub(crate) fn source_policy_json_for_type(body: &Value, source_type: &str) -> String {
    let is_watch = is_watch_source_type(source_type);
    format!(
        "{{\"private_reader_only\":{},\"excerpt_only\":{},\"link_required\":{},\"attribution_required\":{},\"image_allowed\":{},\"full_text_allowed\":{},\"watch\":{},\"public_only\":{},\"no_remote_relationship\":{}}}",
        source_policy_default_true(body, "private_reader_only", "privateReaderOnly") || is_watch,
        source_policy_default_true(body, "excerpt_only", "excerptOnly"),
        source_policy_default_true(body, "link_required", "linkRequired"),
        source_policy_default_true(body, "attribution_required", "attributionRequired"),
        source_policy_bool(body, "image_allowed", "imageAllowed"),
        source_policy_bool(body, "full_text_allowed", "fullTextAllowed"),
        is_watch,
        is_watch,
        is_watch,
    )
}

fn source_policy_default_true(body: &Value, snake: &str, camel: &str) -> bool {
    !matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(false))
    )
}

fn source_policy_bool(body: &Value, snake: &str, camel: &str) -> bool {
    matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(true))
    )
}

pub(crate) fn normalize_source_type(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(':', "_")
}

pub(crate) fn addable_source_types() -> Vec<&'static str> {
    SOURCE_TYPES
        .iter()
        .copied()
        .filter(|value| *value != "activitypub")
        .collect()
}

pub(crate) fn is_addable_source_type(value: &str) -> bool {
    addable_source_types().iter().any(|item| *item == value)
}

pub(crate) fn is_watch_source_type(value: &str) -> bool {
    WATCH_SOURCE_TYPES.iter().any(|item| *item == value)
}

pub(crate) fn is_refreshable_source_type(value: &str) -> bool {
    REFRESHABLE_SOURCE_TYPES.iter().any(|item| *item == value)
}

pub(crate) fn source_type_for_watch_kind(value: &str) -> Option<&'static str> {
    match normalize_source_type(value).as_str() {
        "rss" | "feed" | "watch_rss" => Some("watch_rss"),
        "atom" | "watch_atom" => Some("watch_atom"),
        "activitypub" | "activitypub_actor" | "ap" | "actor" | "watch_activitypub_actor" => {
            Some("watch_activitypub_actor")
        }
        "activitypub_object"
        | "activitypub_post"
        | "ap_object"
        | "ap_post"
        | "watch_activitypub_object" => Some("watch_activitypub_object"),
        "bluesky"
        | "bsky"
        | "atproto"
        | "bluesky_actor"
        | "atproto_actor"
        | "watch_bluesky_actor" => Some("watch_bluesky_actor"),
        "bluesky_post" | "bsky_post" | "atproto_post" | "watch_bluesky_post" => {
            Some("watch_bluesky_post")
        }
        _ => None,
    }
}

pub(crate) fn normalized_source_target(
    source_type: &str,
    body: &Value,
) -> std::result::Result<String, String> {
    let raw = string_like_any(
        body,
        &["url", "target", "uri", "actor", "feed_url", "feedUrl"],
    )
    .unwrap_or_default();
    match source_type {
        "watch_activitypub_actor" => normalized_activitypub_actor_target(&raw),
        "watch_bluesky_actor" => bluesky_actor_target(&raw),
        "watch_bluesky_post" => bluesky_post_uri(&raw),
        "watch_rss" | "watch_atom" | "watch_activitypub_object" => {
            public_https_url(&raw, "watch target")
        }
        _ => public_https_url(&raw, "source url"),
    }
}

fn normalized_activitypub_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with('@') && trimmed.trim_start_matches('@').contains('@') {
        return Ok(trimmed.to_string());
    }
    public_https_url(trimmed, "watch target")
}

pub(crate) fn bluesky_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return Err("watch target is required".to_string());
    }
    if trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    if trimmed.starts_with("at://") {
        return trimmed
            .trim_start_matches("at://")
            .split('/')
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Bluesky actor target is invalid".to_string());
    }
    if let Ok(url) = worker::Url::parse(trimmed) {
        if url.host_str() != Some("bsky.app") {
            return Err("Bluesky actor URL must be on bsky.app".to_string());
        }
        let mut parts = url.path().split('/').filter(|part| !part.is_empty());
        if parts.next() == Some("profile") {
            if let Some(actor) = parts.next().filter(|value| !value.trim().is_empty()) {
                return Ok(actor.to_string());
            }
        }
        return Err(
            "Bluesky actor URL must look like https://bsky.app/profile/<handle-or-did>".to_string(),
        );
    }
    if trimmed.contains('.') || trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    Err("Bluesky actor target must be a handle, DID, or bsky.app profile URL".to_string())
}

pub(crate) fn bluesky_post_uri(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with("at://") && trimmed.contains("/app.bsky.feed.post/") {
        return Ok(trimmed.to_string());
    }
    let url = worker::Url::parse(trimmed)
        .map_err(|_| "Bluesky post target must be an at:// URI or bsky.app post URL".to_string())?;
    if url.host_str() != Some("bsky.app") {
        return Err("Bluesky post URL must be on bsky.app".to_string());
    }
    let parts: Vec<&str> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 4 && parts[0] == "profile" && parts[2] == "post" {
        return Ok(format!("at://{}/app.bsky.feed.post/{}", parts[1], parts[3]));
    }
    Err(
        "Bluesky post URL must look like https://bsky.app/profile/<handle-or-did>/post/<rkey>"
            .to_string(),
    )
}

pub(crate) fn source_id(source_type: &str, source_url: &str) -> String {
    let digest = Sha256::digest(format!("{source_type}\n{source_url}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("source-{}", &hex[..24])
}
