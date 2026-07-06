use crate::request::{optional_body_string, required_body_string};
use crate::{
    activitypub_actor_url_for_target, body_string_any, fetch_activitypub_json,
    fetch_activitypub_json_signed, insert_if_string, owner_local_actor, public_https_url,
    resolve_activitypub_actor_for_local, should_retry_signed_fetch, stable_id, string_field,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use worker::{D1Type, Env, Result};

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

pub(crate) async fn public_e2ee_devices(env: &Env, actor_id: &str) -> Result<Vec<Value>> {
    let actor_arg = D1Type::Text(actor_id);
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT device_id, display_name, protocol, credential, key_package, fingerprint, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1 AND status = 'active'
            ORDER BY updated_at DESC, device_id ASC
            "#,
        )
        .bind_refs(&actor_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut device = Map::new();
            insert_if_string(&mut device, "deviceId", row.get("device_id"));
            insert_if_string(&mut device, "displayName", row.get("display_name"));
            insert_if_string(&mut device, "protocol", row.get("protocol"));
            insert_if_string(&mut device, "credential", row.get("credential"));
            insert_if_string(&mut device, "keyPackage", row.get("key_package"));
            insert_if_string(&mut device, "fingerprint", row.get("fingerprint"));
            insert_if_string(&mut device, "updatedAt", row.get("updated_at"));
            Value::Object(device)
        })
        .collect())
}

pub(crate) async fn owner_e2ee_devices(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let local_actor = owner_local_actor(env).await?;
    let actor_arg = D1Type::Text(&local_actor.id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, status, created_at, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1
            ORDER BY updated_at DESC, device_id ASC
            "#,
        )
        .bind_refs(&actor_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()
}

pub(crate) async fn owner_upsert_e2ee_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let display_name = body_string_any(body, &["display_name", "displayName"]);
    let protocol = normalize_e2ee_protocol(
        body_string_any(body, &["protocol"])
            .unwrap_or_else(|| "dais-mls-v1".to_string())
            .as_str(),
    )?;
    let credential = required_e2ee_material(body, &["credential", "identityKey"], "credential")?;
    let key_package = required_e2ee_material(body, &["key_package", "keyPackage"], "keyPackage")?;
    validate_e2ee_device_material(&protocol, &credential, &key_package)?;
    let fingerprint = e2ee_device_fingerprint(&credential, &key_package);
    let row_id = format!(
        "e2ee-device-{}",
        stable_id(&format!("{}\n{}", local_actor.id, device_id))
    );
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&row_id);
    let actor_arg = D1Type::Text(&local_actor.id);
    let device_arg = D1Type::Text(&device_id);
    let display_arg = display_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let protocol_arg = D1Type::Text(&protocol);
    let credential_arg = D1Type::Text(&credential);
    let key_package_arg = D1Type::Text(&key_package);
    let fingerprint_arg = D1Type::Text(&fingerprint);
    db.prepare(
        r#"
        INSERT INTO e2ee_devices (
            id, actor_id, device_id, display_name, protocol, credential, key_package,
            fingerprint, status, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', datetime('now'), datetime('now')
        )
        ON CONFLICT(actor_id, device_id) DO UPDATE SET
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            credential = excluded.credential,
            key_package = excluded.key_package,
            fingerprint = excluded.fingerprint,
            status = 'active',
            updated_at = datetime('now')
        "#,
    )
    .bind_refs(&[
        id_arg,
        actor_arg,
        device_arg,
        display_arg,
        protocol_arg,
        credential_arg,
        key_package_arg,
        fingerprint_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "device not found after upsert".to_string())
}

pub(crate) async fn owner_revoke_e2ee_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let actor_arg = D1Type::Text(&local_actor.id);
    let device_arg = D1Type::Text(&device_id);
    env.d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            UPDATE e2ee_devices
            SET status = 'revoked', updated_at = datetime('now')
            WHERE actor_id = ?1 AND device_id = ?2
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "device not found".to_string())
}

pub(crate) async fn owner_e2ee_peer_devices(env: &Env) -> Result<Vec<Map<String, Value>>> {
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
            FROM e2ee_peer_devices
            ORDER BY last_seen_at DESC, actor_id ASC, device_id ASC
            "#,
        )
        .all()
        .await?
        .results::<Map<String, Value>>()
}

pub(crate) async fn owner_discover_e2ee_peer_devices(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let target = body_string_any(body, &["actor_id", "actorId", "actor", "target"])
        .ok_or("actorId is required")?;
    let actor_url = activitypub_actor_url_for_target(&target).await?;
    let actor = match fetch_activitypub_json(&actor_url, "actor").await {
        Ok(actor) => actor,
        Err(unsigned_error)
            if should_retry_signed_fetch(&unsigned_error) && local_actor.can_sign() =>
        {
            fetch_activitypub_json_signed(&actor_url, "actor", &local_actor)
                .await
                .map_err(|signed_error| {
                    format!("{unsigned_error}; signed retry failed: {signed_error}")
                })?
        }
        Err(error) => return Err(error),
    };
    let actor_id = actor
        .get("id")
        .and_then(optional_body_string)
        .unwrap_or(actor_url);
    public_https_url(&actor_id, "actorId")?;
    let devices = actor
        .get("daisE2ee")
        .and_then(|value| value.get("devices"))
        .and_then(Value::as_array)
        .ok_or("actor does not publish daisE2ee.devices")?;
    if devices.is_empty() {
        return Err("actor publishes no E2EE devices".to_string());
    }

    let mut rows = Vec::new();
    for device in devices {
        let Some(device) = device.as_object() else {
            return Err("daisE2ee device must be an object".to_string());
        };
        let mut peer = Map::new();
        peer.insert("actorId".to_string(), Value::String(actor_id.clone()));
        copy_e2ee_device_field(device, &mut peer, "deviceId", "deviceId")?;
        copy_e2ee_device_field(device, &mut peer, "credential", "credential")?;
        copy_e2ee_device_field(device, &mut peer, "keyPackage", "keyPackage")?;
        copy_optional_e2ee_device_field(device, &mut peer, "displayName", "displayName");
        copy_optional_e2ee_device_field(device, &mut peer, "protocol", "protocol");
        copy_optional_e2ee_device_field(device, &mut peer, "fingerprint", "fingerprint");
        let row =
            owner_upsert_peer_device_with_trust(env, &Value::Object(peer), "untrusted").await?;
        rows.push(Value::Object(row));
    }

    let mut result = Map::new();
    result.insert("actor_id".to_string(), Value::String(actor_id));
    result.insert("items".to_string(), Value::Array(rows));
    Ok(result)
}

fn copy_e2ee_device_field(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
) -> std::result::Result<(), String> {
    let value = source
        .get(source_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("daisE2ee device missing {source_key}"))?;
    target.insert(target_key.to_string(), Value::String(value.to_string()));
    Ok(())
}

fn copy_optional_e2ee_device_field(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
) {
    if let Some(value) = source
        .get(source_key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        target.insert(target_key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) async fn owner_trust_e2ee_peer_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    owner_upsert_peer_device_with_trust(env, body, "trusted").await
}

pub(crate) async fn owner_revoke_e2ee_peer_device(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let actor_id = public_https_url(
        &body_string_any(body, &["actor_id", "actorId", "actor"]).ok_or("actorId is required")?,
        "actorId",
    )?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let actor_arg = D1Type::Text(&actor_id);
    let device_arg = D1Type::Text(&device_id);
    env.d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            UPDATE e2ee_peer_devices
            SET trust_state = 'revoked', revoked_at = datetime('now'), last_seen_at = datetime('now')
            WHERE actor_id = ?1 AND device_id = ?2
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])
        .map_err(|error| error.to_string())?
        .run()
        .await
        .map_err(|error| error.to_string())?;
    owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "peer device not found".to_string())
}

async fn owner_upsert_peer_device_with_trust(
    env: &Env,
    body: &Value,
    trust_state: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let actor_id = public_https_url(
        &body_string_any(body, &["actor_id", "actorId", "actor"]).ok_or("actorId is required")?,
        "actorId",
    )?;
    let device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["device_id", "deviceId"]).ok_or("deviceId is required")?,
    )?;
    let display_name = body_string_any(body, &["display_name", "displayName"]);
    let protocol = normalize_e2ee_protocol(
        body_string_any(body, &["protocol"])
            .unwrap_or_else(|| "dais-mls-v1".to_string())
            .as_str(),
    )?;
    let credential = required_e2ee_material(body, &["credential", "identityKey"], "credential")?;
    let key_package = required_e2ee_material(body, &["key_package", "keyPackage"], "keyPackage")?;
    validate_e2ee_device_material(&protocol, &credential, &key_package)?;
    let fingerprint = body_string_any(body, &["fingerprint"])
        .map(|value| normalize_e2ee_fingerprint(&value))
        .transpose()?
        .unwrap_or_else(|| e2ee_device_fingerprint(&credential, &key_package));
    if fingerprint != e2ee_device_fingerprint(&credential, &key_package) {
        return Err("fingerprint does not match credential and keyPackage".to_string());
    }
    let existing = owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?;
    let existing_fingerprint = existing
        .as_ref()
        .and_then(|row| string_field(Some(row), "fingerprint"));
    let existing_trust_state = existing
        .as_ref()
        .and_then(|row| string_field(Some(row), "trust_state"));
    let effective_trust_state = peer_trust_state_after_material_update(
        existing_fingerprint.as_deref(),
        existing_trust_state.as_deref(),
        trust_state,
        &fingerprint,
    );
    let row_id = format!(
        "e2ee-peer-{}",
        stable_id(&format!("{}\n{}", actor_id, device_id))
    );
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&row_id);
    let actor_arg = D1Type::Text(&actor_id);
    let device_arg = D1Type::Text(&device_id);
    let display_arg = display_name
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let protocol_arg = D1Type::Text(&protocol);
    let credential_arg = D1Type::Text(&credential);
    let key_package_arg = D1Type::Text(&key_package);
    let fingerprint_arg = D1Type::Text(&fingerprint);
    let trust_arg = D1Type::Text(effective_trust_state);
    db.prepare(
        r#"
        INSERT INTO e2ee_peer_devices (
            id, actor_id, device_id, display_name, protocol, credential, key_package,
            fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), datetime('now'),
            CASE WHEN ?9 = 'trusted' THEN datetime('now') ELSE NULL END, NULL
        )
        ON CONFLICT(actor_id, device_id) DO UPDATE SET
            display_name = excluded.display_name,
            protocol = excluded.protocol,
            credential = excluded.credential,
            key_package = excluded.key_package,
            fingerprint = excluded.fingerprint,
            trust_state = excluded.trust_state,
            last_seen_at = datetime('now'),
            trusted_at = CASE
                WHEN excluded.trust_state = 'trusted' THEN datetime('now')
                WHEN e2ee_peer_devices.fingerprint != excluded.fingerprint THEN NULL
                ELSE trusted_at
            END,
            revoked_at = CASE WHEN excluded.trust_state = 'revoked' THEN datetime('now') ELSE NULL END
        "#,
    )
    .bind_refs(&[
        id_arg,
        actor_arg,
        device_arg,
        display_arg,
        protocol_arg,
        credential_arg,
        key_package_arg,
        fingerprint_arg,
        trust_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    owner_e2ee_peer_device_by_actor_and_device(env, &actor_id, &device_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "peer device not found after upsert".to_string())
}

pub(crate) async fn owner_e2ee_device_by_actor_and_device(
    env: &Env,
    actor_id: &str,
    device_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let actor_arg = D1Type::Text(actor_id);
    let device_arg = D1Type::Text(device_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, status, created_at, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1 AND device_id = ?2
            LIMIT 1
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])?
        .first::<Map<String, Value>>(None)
        .await
}

pub(crate) async fn owner_e2ee_peer_device_by_actor_and_device(
    env: &Env,
    actor_id: &str,
    device_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let actor_arg = D1Type::Text(actor_id);
    let device_arg = D1Type::Text(device_id);
    env.d1("DB")?
        .prepare(
            r#"
            SELECT id, actor_id, device_id, display_name, protocol, credential, key_package,
                   fingerprint, trust_state, first_seen_at, last_seen_at, trusted_at, revoked_at
            FROM e2ee_peer_devices
            WHERE actor_id = ?1 AND device_id = ?2
            LIMIT 1
            "#,
        )
        .bind_refs(&[actor_arg, device_arg])?
        .first::<Map<String, Value>>(None)
        .await
}

pub(crate) async fn owner_require_trusted_e2ee_peer(
    env: &Env,
    actor_id: &str,
    device_id: Option<&str>,
) -> std::result::Result<(), String> {
    if let Some(device_id) = device_id {
        let device_id = normalize_e2ee_device_id(device_id)?;
        let Some(peer) = owner_e2ee_peer_device_by_actor_and_device(env, actor_id, &device_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Err("recipientDeviceId is not known".to_string());
        };
        if string_field(Some(&peer), "trust_state").as_deref() == Some("trusted") {
            return Ok(());
        }
        return Err("recipientDeviceId is not trusted".to_string());
    }

    let actor_arg = D1Type::Text(actor_id);
    let trusted = env
        .d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            SELECT id
            FROM e2ee_peer_devices
            WHERE actor_id = ?1 AND trust_state = 'trusted'
            LIMIT 1
            "#,
        )
        .bind_refs(&actor_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    trusted
        .map(|_| ())
        .ok_or_else(|| "recipient has no trusted E2EE device".to_string())
}

pub(crate) async fn owner_e2ee_inbox_for_actor(
    env: &Env,
    actor_id: &str,
) -> std::result::Result<String, String> {
    let actor_arg = D1Type::Text(actor_id);
    let row = env
        .d1("DB")
        .map_err(|error| error.to_string())?
        .prepare(
            r#"
            SELECT inbox FROM (
                SELECT follower_inbox AS inbox, 0 AS rank
                FROM followers
                WHERE follower_actor_id = ?1 AND status = 'approved'
                UNION ALL
                SELECT target_inbox AS inbox, 1 AS rank
                FROM following
                WHERE target_actor_id = ?1 AND status IN ('accepted', 'pending')
            )
            WHERE inbox IS NOT NULL AND inbox <> ''
            ORDER BY rank ASC
            LIMIT 1
            "#,
        )
        .bind_refs(&actor_arg)
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(inbox) = row
        .as_ref()
        .and_then(|row| string_field(Some(row), "inbox"))
    {
        Ok(inbox)
    } else {
        async_resolve_e2ee_actor_inbox(env, actor_id).await
    }
}

async fn async_resolve_e2ee_actor_inbox(
    env: &Env,
    actor_id: &str,
) -> std::result::Result<String, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let actor = resolve_activitypub_actor_for_local(actor_id, &local_actor).await?;
    let inbox = actor.shared_inbox.unwrap_or(actor.inbox);
    if inbox.trim().is_empty() {
        return Err("recipient actor does not expose an inbox".to_string());
    }
    public_https_url(&inbox, "recipient inbox")
}
