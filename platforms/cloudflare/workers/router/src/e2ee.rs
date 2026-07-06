use crate::request::required_body_string;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub(crate) fn normalize_e2ee_device_id(value: &str) -> std::result::Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err("deviceId is required".to_string());
    }
    if normalized.len() > 128 {
        return Err("deviceId is too long".to_string());
    }
    if !normalized
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(
            "deviceId may only contain letters, numbers, dot, colon, dash, and underscore"
                .to_string(),
        );
    }
    Ok(normalized.to_string())
}

pub(crate) fn normalize_e2ee_protocol(value: &str) -> std::result::Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "dais-mls-v1" | "encryptedmessage-v1" | "encrypted-message-v1" => {
            Ok("dais-mls-v1".to_string())
        }
        "mls" | "openmls" | "mls-rfc9420" | "openmls-rfc9420" | "dais-mls-v2" => {
            Ok("mls-rfc9420".to_string())
        }
        _ => Err("unsupported E2EE protocol".to_string()),
    }
}

pub(crate) fn normalize_e2ee_fingerprint(value: &str) -> std::result::Result<String, String> {
    let normalized = value
        .trim()
        .trim_start_matches("sha256:")
        .replace([':', ' ', '-'], "")
        .to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("fingerprint must be a SHA-256 hex digest".to_string());
    }
    Ok(normalized)
}

pub(crate) fn required_e2ee_material(
    body: &Value,
    keys: &[&str],
    field: &str,
) -> std::result::Result<String, String> {
    let value = e2ee_body_string_any(body, keys).ok_or_else(|| format!("{field} is required"))?;
    if value.len() > 65536 {
        return Err(format!("{field} is too large"));
    }
    Ok(value)
}

fn e2ee_body_string_any(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| required_body_string(body.get(*key)))
}

pub(crate) fn validate_e2ee_device_material(
    protocol: &str,
    credential: &str,
    key_package: &str,
) -> std::result::Result<(), String> {
    if protocol == "mls-rfc9420" {
        let credential_bytes = BASE64
            .decode(credential.as_bytes())
            .map_err(|_| "MLS credential must be base64".to_string())?;
        let key_package_bytes = BASE64
            .decode(key_package.as_bytes())
            .map_err(|_| "MLS keyPackage must be base64".to_string())?;
        if credential_bytes.is_empty() {
            return Err("MLS credential must not be empty".to_string());
        }
        if key_package_bytes.is_empty() {
            return Err("MLS keyPackage must not be empty".to_string());
        }
    }
    Ok(())
}

pub(crate) fn validate_owner_e2ee_payload(
    value: &Value,
) -> std::result::Result<(&'static str, &'static str), String> {
    if value.get("protocol").and_then(Value::as_str) == Some("mls-rfc9420")
        || value.get("v").and_then(Value::as_u64) == Some(2)
    {
        validate_dais_encrypted_message_v2(value)?;
        Ok(("daisEncryptedMessage", "mls-rfc9420"))
    } else {
        validate_encrypted_message_envelope(value)?;
        Ok(("encryptedMessage", "dais-mls-v1"))
    }
}

pub(crate) fn validate_dais_encrypted_message_v2(value: &Value) -> std::result::Result<(), String> {
    let envelope = value
        .as_object()
        .ok_or_else(|| "daisEncryptedMessage must be an object".to_string())?;
    match envelope.get("v").and_then(Value::as_u64) {
        Some(2) => {}
        Some(version) => {
            return Err(format!(
                "unsupported daisEncryptedMessage version {version}"
            ))
        }
        None => return Err("daisEncryptedMessage.v is required".to_string()),
    }
    match envelope.get("protocol").and_then(Value::as_str) {
        Some("mls-rfc9420") => {}
        Some(_) => return Err("daisEncryptedMessage.protocol must be mls-rfc9420".to_string()),
        None => return Err("daisEncryptedMessage.protocol is required".to_string()),
    }
    required_nonempty_string(envelope, "groupId", "daisEncryptedMessage", 512)?;
    let epoch = envelope
        .get("epoch")
        .and_then(Value::as_u64)
        .ok_or_else(|| "daisEncryptedMessage.epoch is required".to_string())?;
    if epoch > i32::MAX as u64 {
        return Err("daisEncryptedMessage.epoch is too large".to_string());
    }
    normalize_e2ee_device_id(&required_nonempty_string(
        envelope,
        "senderDeviceId",
        "daisEncryptedMessage",
        128,
    )?)?;
    if required_dais_mls_base64(envelope, "ciphertext")?.is_empty() {
        return Err("daisEncryptedMessage.ciphertext must not be empty".to_string());
    }
    Ok(())
}

fn required_nonempty_string(
    object: &Map<String, Value>,
    key: &str,
    prefix: &str,
    max_len: usize,
) -> std::result::Result<String, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{prefix}.{key} is required"))?;
    if value.len() > max_len {
        return Err(format!("{prefix}.{key} is too long"));
    }
    Ok(value.to_string())
}

fn required_dais_mls_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = required_nonempty_string(object, key, "daisEncryptedMessage", 262144)?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("daisEncryptedMessage.{key} must be valid base64"))
}

pub(crate) fn validate_encrypted_message_envelope(
    value: &Value,
) -> std::result::Result<(), String> {
    let envelope = value
        .as_object()
        .ok_or_else(|| "encryptedMessage must be an object".to_string())?;
    match envelope.get("v").and_then(Value::as_u64) {
        Some(1) => {}
        Some(version) => return Err(format!("unsupported encryptedMessage version {version}")),
        None => return Err("encryptedMessage.v is required".to_string()),
    }
    match envelope.get("alg").and_then(Value::as_str) {
        Some("AES-256-GCM") => {}
        Some(_) => return Err("encryptedMessage.alg must be AES-256-GCM".to_string()),
        None => return Err("encryptedMessage.alg is required".to_string()),
    }
    match envelope.get("keyWrap").and_then(Value::as_str) {
        Some("RSA-OAEP-256") | Some("RSA-OAEP-SHA256") => {}
        Some(_) => {
            return Err(
                "encryptedMessage.keyWrap must be RSA-OAEP-256 or RSA-OAEP-SHA256".to_string(),
            )
        }
        None => return Err("encryptedMessage.keyWrap is required".to_string()),
    }
    let iv = required_encrypted_base64(envelope, "iv")?;
    if iv.len() != 12 {
        return Err("encryptedMessage.iv must decode to 12 bytes".to_string());
    }
    if required_encrypted_base64(envelope, "ciphertext")?.is_empty() {
        return Err("encryptedMessage.ciphertext must not be empty".to_string());
    }
    let recipients = envelope
        .get("recipients")
        .and_then(Value::as_array)
        .ok_or_else(|| "encryptedMessage.recipients must be an array".to_string())?;
    if recipients.is_empty() {
        return Err("encryptedMessage must include at least one recipient".to_string());
    }
    for recipient in recipients {
        let recipient = recipient
            .as_object()
            .ok_or_else(|| "encryptedMessage recipient must be an object".to_string())?;
        let key_id = recipient
            .get("keyId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "encryptedMessage recipient keyId is required".to_string())?;
        if key_id.len() > 512 {
            return Err("encryptedMessage recipient keyId is too long".to_string());
        }
        if required_encrypted_base64(recipient, "wrappedKey")?.is_empty() {
            return Err("encryptedMessage recipient wrappedKey must not be empty".to_string());
        }
    }
    Ok(())
}

pub(crate) fn validate_encrypted_media_payload(value: &Value) -> std::result::Result<(), String> {
    let payload = value
        .as_object()
        .ok_or_else(|| "encryptedMedia must be an object".to_string())?;
    match payload.get("v").and_then(Value::as_u64) {
        Some(1) => {}
        Some(version) => return Err(format!("unsupported encryptedMedia version {version}")),
        None => return Err("encryptedMedia.v is required".to_string()),
    }
    match payload.get("alg").and_then(Value::as_str) {
        Some("AES-256-GCM") => {}
        Some(_) => return Err("encryptedMedia.alg must be AES-256-GCM".to_string()),
        None => return Err("encryptedMedia.alg is required".to_string()),
    }
    let iv = required_encrypted_media_base64(payload, "iv")?;
    if iv.len() != 12 {
        return Err("encryptedMedia.iv must decode to 12 bytes".to_string());
    }
    if required_encrypted_media_base64(payload, "ciphertext")?.is_empty() {
        return Err("encryptedMedia.ciphertext must not be empty".to_string());
    }
    Ok(())
}

fn required_encrypted_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("encryptedMessage.{key} is required"))?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("encryptedMessage.{key} must be valid base64"))
}

fn required_encrypted_media_base64(
    object: &Map<String, Value>,
    key: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("encryptedMedia.{key} is required"))?;
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("encryptedMedia.{key} must be valid base64"))
}

pub(crate) fn e2ee_device_fingerprint(credential: &str, key_package: &str) -> String {
    let digest = Sha256::digest(format!("{credential}\n{key_package}").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn peer_trust_state_after_material_update<'a>(
    existing_fingerprint: Option<&str>,
    existing_trust_state: Option<&'a str>,
    requested_trust_state: &'a str,
    new_fingerprint: &str,
) -> &'a str {
    if requested_trust_state == "trusted" {
        return "trusted";
    }
    if requested_trust_state == "revoked" {
        return "revoked";
    }
    match existing_fingerprint {
        Some(existing) if existing != new_fingerprint => "untrusted",
        Some(_) if existing_trust_state == Some("trusted") => "trusted",
        _ => requested_trust_state,
    }
}
