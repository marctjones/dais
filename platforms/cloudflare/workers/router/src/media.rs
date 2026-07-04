use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
