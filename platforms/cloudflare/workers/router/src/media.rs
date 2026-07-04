use sha2::{Digest, Sha256};
use std::collections::HashMap;

use serde_json::Value;

pub(crate) struct MediaMetadataInput<'a> {
    pub(crate) owner: &'a str,
    pub(crate) access: &'a str,
    pub(crate) media_type: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) created_at: &'a str,
    pub(crate) description: Option<&'a str>,
    pub(crate) expires_at: Option<&'a str>,
    pub(crate) require_authorized_fetch: bool,
}

pub(crate) fn media_type_for_filename(filename: &str) -> String {
    match filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub(crate) fn media_r2_key_from_path(path: &str) -> Option<String> {
    path.strip_prefix("/media/_private_signed/")
        .or_else(|| path.strip_prefix("/media/_private/"))
        .map(|rest| format!("private/{}", decode_component(rest)))
        .or_else(|| {
            path.strip_prefix("/media/uploads/")
                .map(|rest| decode_component(&format!("uploads/{rest}")))
        })
        .filter(|key| !key.trim().is_empty() && !key.contains(".."))
}

pub(crate) fn media_r2_key_from_url(value: &str) -> Option<String> {
    let parsed = worker::Url::parse(value).ok()?;
    if !is_known_activitypub_host(parsed.host_str()) {
        return None;
    }
    let path = parsed.path();
    if let Some(rest) = path.strip_prefix("/media/_private/") {
        return Some(format!("private/{}", decode_component(rest)));
    }
    if let Some(rest) = path.strip_prefix("/media/_private_signed/") {
        return Some(format!("private/{}", decode_component(rest)));
    }
    if let Some(rest) = path.strip_prefix("/media/uploads/") {
        return Some(decode_component(&format!("uploads/{rest}")));
    }
    None
}

pub(crate) fn is_private_media_attachment(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("url"))
        .and_then(Value::as_str)
        .and_then(|url| worker::Url::parse(url).ok())
        .map(|url| {
            is_known_activitypub_host(url.host_str())
                && (url.path().starts_with("/media/_private/")
                    || url.path().starts_with("/media/_private_signed/"))
        })
        .unwrap_or(false)
}

pub(crate) fn is_public_atproto_image_attachment(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let media_type_is_image = object
        .get("mediaType")
        .and_then(Value::as_str)
        .map(|value| value.starts_with("image/"))
        .unwrap_or(false);
    if !media_type_is_image {
        return false;
    }
    !is_private_media_attachment(value)
        && object
            .get("url")
            .and_then(Value::as_str)
            .and_then(|url| worker::Url::parse(url).ok())
            .is_some_and(|url| url.scheme() == "https")
}

pub(crate) fn media_custom_metadata(input: MediaMetadataInput<'_>) -> HashMap<String, String> {
    let mut custom_metadata = HashMap::new();
    custom_metadata.insert("owner".to_string(), input.owner.to_string());
    custom_metadata.insert("visibility".to_string(), input.access.to_string());
    custom_metadata.insert("media_type".to_string(), input.media_type.to_string());
    custom_metadata.insert("size".to_string(), input.bytes.len().to_string());
    custom_metadata.insert("sha256".to_string(), sha256_hex(input.bytes));
    custom_metadata.insert("created_at".to_string(), input.created_at.to_string());
    if let Some(description) = input.description {
        custom_metadata.insert("description".to_string(), description.to_string());
    }
    if let Some(expires_at) = input.expires_at {
        custom_metadata.insert("expires_at".to_string(), expires_at.to_string());
    }
    if input.require_authorized_fetch {
        custom_metadata.insert("authorized_fetch".to_string(), "required".to_string());
    }
    custom_metadata
}

pub(crate) fn media_metadata_is_expired(metadata: &HashMap<String, String>, now_ms: f64) -> bool {
    let Some(expires_at) = metadata.get("expires_at").map(String::as_str) else {
        return false;
    };
    let expires_ms = js_sys::Date::parse(expires_at);
    expires_ms.is_finite() && expires_ms <= now_ms
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn is_known_activitypub_host(host: Option<&str>) -> bool {
    matches!(host, Some("social.dais.social") | Some("social.skpt.cl"))
}
