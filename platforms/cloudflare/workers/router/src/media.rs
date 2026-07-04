use sha2::{Digest, Sha256};
use std::collections::HashMap;
use wasm_bindgen::{JsCast, JsValue};
use worker::{Env, Headers, Request, Response, Result};

use serde_json::{Map, Value};

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

pub(crate) async fn handle_media(req: Request, env: Env, url: &worker::Url) -> Result<Response> {
    let path = url.path();
    let Some(key) = media_r2_key_from_path(path) else {
        return Response::error("Not found", 404);
    };
    if path.starts_with("/media/_private_signed/") {
        if req.headers().get("Signature")?.is_none() {
            return Response::error("HTTP Signature required", 401);
        }
        if !crate::signed_approved_follower(&env, &req).await? {
            return Response::error("Signed media fetch requires an approved follower", 403);
        }
        if !private_media_attached_post(&env, &crate::origin(url), path).await? {
            return Response::error("Not found", 404);
        }
    } else if path.starts_with("/media/_private/") {
        return Response::error("HTTP Signature required", 401);
    }

    let bucket = env.bucket("MEDIA_BUCKET")?;
    let Some(object) = bucket.get(key.clone()).execute().await? else {
        return Response::error("Not found", 404);
    };
    let custom_metadata = object.custom_metadata()?;
    if media_metadata_is_expired(&custom_metadata, js_sys::Date::now()) {
        bucket.delete(key).await?;
        return Response::error("Not found", 404);
    }
    let bytes = match object.body() {
        Some(body) => body.bytes().await?,
        None => Vec::new(),
    };
    let mut response = Response::from_bytes(bytes)?;
    let headers = Headers::new();
    headers.set(
        "Content-Type",
        &object
            .http_metadata()
            .content_type
            .unwrap_or_else(|| media_type_for_filename(&key)),
    )?;
    headers.set("Cache-Control", "private, max-age=300")?;
    response = response.with_headers(headers);
    Ok(response)
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

pub(crate) fn allowed_media_type(value: &str) -> bool {
    matches!(
        value,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "video/mp4" | "video/webm"
    )
}

pub(crate) fn safe_media_filename(value: &str) -> std::result::Result<String, String> {
    let basename = value.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let mut safe = String::new();
    let mut previous_dash = false;
    for ch in basename.chars() {
        let replacement = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            ch
        } else {
            '-'
        };
        if replacement == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        safe.push(replacement);
    }
    let safe = safe
        .trim_start_matches('.')
        .chars()
        .take(96)
        .collect::<String>();
    if safe.is_empty() {
        return Err("filename is invalid".to_string());
    }
    Ok(safe)
}

pub(crate) fn private_media_expires_at(
    value: Option<&Value>,
) -> std::result::Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() || matches!(value, Value::String(text) if text.is_empty()) {
        return Ok(None);
    }
    let seconds = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
    .ok_or_else(|| "expires_in_seconds must be a positive number".to_string())?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("expires_in_seconds must be a positive number".to_string());
    }
    if seconds > 30.0 * 24.0 * 60.0 * 60.0 {
        return Err("expires_in_seconds must be 30 days or less".to_string());
    }
    let expires_ms = js_sys::Date::now() + seconds.floor() * 1000.0;
    Ok(js_sys::Date::new(&JsValue::from_f64(expires_ms))
        .to_iso_string()
        .as_string())
}

pub(crate) fn current_media_timestamp() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
        .chars()
        .filter(|ch| !matches!(ch, '-' | ':' | 'T' | 'Z' | '.'))
        .take(14)
        .collect()
}

pub(crate) fn current_media_created_at() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

pub(crate) fn random_token() -> std::result::Result<String, String> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| "crypto is unavailable".to_string())?;
    let get_random_values = js_sys::Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .map_err(|_| "crypto.getRandomValues is unavailable".to_string())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "crypto.getRandomValues is unavailable".to_string())?;
    let array = js_sys::Uint8Array::new_with_length(24);
    get_random_values
        .call1(&crypto, &array)
        .map_err(|_| "crypto.getRandomValues failed".to_string())?;
    let mut bytes = vec![0; 24];
    array.copy_to(&mut bytes);
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
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

async fn private_media_attached_post(
    env: &Env,
    request_origin: &str,
    media_path: &str,
) -> Result<bool> {
    let media_url = format!("{request_origin}{media_path}");
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT media_attachments
            FROM posts
            WHERE visibility IN ('followers', 'direct')
              AND media_attachments IS NOT NULL
              AND media_attachments != ''
            ORDER BY published_at DESC
            LIMIT 250
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    for row in rows {
        for attachment in crate::parse_attachment_array(row.get("media_attachments")) {
            let Some(object) = attachment.as_object() else {
                continue;
            };
            if crate::string_field(Some(object), "url").as_deref() == Some(media_url.as_str()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn is_known_activitypub_host(host: Option<&str>) -> bool {
    matches!(host, Some("social.dais.social") | Some("social.skpt.cl"))
}
