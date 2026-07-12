use crate::deliveries::{insert_delivery_rows, owner_delivery_rows_for_post};
use crate::request::{optional_body_string, required_body_string};
use crate::{
    activitypub_actor_url_for_target, body_string_any, fetch_activitypub_json,
    fetch_activitypub_json_signed, insert_if_string, normalize_encrypted_media_attachments,
    owner_local_actor, persist_mls_message_metadata, public_https_url,
    resolve_activitypub_actor_for_local, row_value_or_null, should_retry_signed_fetch, stable_id,
    string_field, string_value_or_default, string_vec_json_field, timestamp_for_local_id,
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
    validate_dais_encrypted_message_v2(value)?;
    Ok(("daisEncryptedMessage", "mls-rfc9420"))
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

pub(crate) async fn owner_direct_messages(
    env: &Env,
    limit: i32,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, conversation_id, sender_id, content, published_at, created_at
        FROM direct_messages
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

pub(crate) async fn owner_e2ee_messages(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let local_actor = owner_local_actor(env).await?;
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT m.id, m.conversation_id, m.sender_actor_id, m.sender_device_id,
                   m.ciphertext, m.aad, m.created_at, c.participants, c.protocol
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            WHERE c.protocol = 'mls-rfc9420'
              AND json_valid(m.ciphertext)
              AND json_extract(m.ciphertext, '$.v') = 2
              AND json_extract(m.ciphertext, '$.protocol') = 'mls-rfc9420'
            ORDER BY m.created_at DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let mut items = Vec::new();
    for row in rows {
        items.push(owner_e2ee_message_row(env, &local_actor.id, row).await?);
    }
    Ok(items)
}

pub(crate) async fn owner_send_e2ee_message(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let local_actor = owner_local_actor(env)
        .await
        .map_err(|error| error.to_string())?;
    let recipient_actor_id = public_https_url(
        &body_string_any(
            body,
            &["recipient_actor_id", "recipientActorId", "recipient"],
        )
        .ok_or("recipientActorId is required")?,
        "recipientActorId",
    )?;
    if recipient_actor_id == local_actor.id {
        return Err("recipientActorId must be a remote actor".to_string());
    }
    let sender_device_id = normalize_e2ee_device_id(
        &body_string_any(body, &["sender_device_id", "senderDeviceId"])
            .ok_or("senderDeviceId is required")?,
    )?;
    let encrypted_message = body
        .get("dais_encrypted_message")
        .or_else(|| body.get("daisEncryptedMessage"))
        .cloned()
        .ok_or("daisEncryptedMessage is required; encryptedMessage v1 is no longer supported")?;
    let (envelope_field, protocol) = validate_owner_e2ee_payload(&encrypted_message)?;
    let envelope_sender = encrypted_message
        .get("senderDeviceId")
        .and_then(Value::as_str)
        .ok_or("daisEncryptedMessage.senderDeviceId is required")?;
    if normalize_e2ee_device_id(envelope_sender)? != sender_device_id {
        return Err("senderDeviceId must match daisEncryptedMessage.senderDeviceId".to_string());
    }
    let fallback_content = body_string_any(body, &["fallback_content", "fallbackContent"])
        .unwrap_or_else(|| "Encrypted message. Open in a dais client to decrypt.".to_string());
    if fallback_content.len() > 512 {
        return Err("fallbackContent is too long".to_string());
    }
    let attachments = normalize_encrypted_media_attachments(
        &body
            .get("attachments")
            .or_else(|| body.get("media_attachments"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )?;
    if !attachments.is_empty() {
        return Err("encrypted media attachments must use MLS v2 media envelopes; encryptedMessage v1 attachments are no longer supported".to_string());
    }

    let local_device =
        owner_e2ee_device_by_actor_and_device(env, &local_actor.id, &sender_device_id)
            .await
            .map_err(|error| error.to_string())?;
    match local_device
        .as_ref()
        .and_then(|row| string_field(Some(row), "status"))
        .as_deref()
    {
        Some("active") => {}
        _ => return Err("senderDeviceId is not an active local E2EE device".to_string()),
    }
    if local_device
        .as_ref()
        .and_then(|row| string_field(Some(row), "protocol"))
        .as_deref()
        != Some("mls-rfc9420")
    {
        return Err("senderDeviceId must be an active MLS E2EE device".to_string());
    }

    let recipient_device_id = body_string_any(body, &["recipient_device_id", "recipientDeviceId"]);
    owner_require_trusted_e2ee_peer(env, &recipient_actor_id, recipient_device_id.as_deref())
        .await?;
    let inbox = owner_e2ee_inbox_for_actor(env, &recipient_actor_id).await?;

    let mut participants = vec![local_actor.id.clone(), recipient_actor_id.clone()];
    participants.sort();
    let participants_json =
        serde_json::to_string(&participants).map_err(|error| error.to_string())?;
    let conversation_id = format!("e2ee-conversation-{}", stable_id(&participants.join("\n")));
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let message_id = format!(
        "{}/e2ee/messages/{}-{}",
        local_actor.id,
        timestamp_for_local_id(&now),
        stable_id(&serde_json::to_string(&encrypted_message).unwrap_or_default())
    );
    let aad = serde_json::json!({
        "recipientActorId": recipient_actor_id,
        "fallbackContent": fallback_content,
        "e2eeProtocol": protocol,
        "e2eeField": envelope_field,
        "attachments": attachments.clone(),
    });
    let aad_json = serde_json::to_string(&aad).map_err(|error| error.to_string())?;
    let ciphertext_json =
        serde_json::to_string(&encrypted_message).map_err(|error| error.to_string())?;

    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let conversation_arg = D1Type::Text(&conversation_id);
    let participants_arg = D1Type::Text(&participants_json);
    db.prepare(
        r#"
        INSERT INTO e2ee_conversations (id, protocol, participants, created_at, updated_at)
        VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
        ON CONFLICT(id) DO UPDATE SET protocol = excluded.protocol, updated_at = datetime('now')
        "#,
    )
    .bind_refs(&[conversation_arg, D1Type::Text(protocol), participants_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    let id_arg = D1Type::Text(&message_id);
    let conversation_arg = D1Type::Text(&conversation_id);
    let sender_actor_arg = D1Type::Text(&local_actor.id);
    let sender_device_arg = D1Type::Text(&sender_device_id);
    let ciphertext_arg = D1Type::Text(&ciphertext_json);
    let aad_arg = D1Type::Text(&aad_json);
    db.prepare(
        r#"
        INSERT INTO e2ee_messages (
            id, conversation_id, sender_actor_id, sender_device_id, ciphertext, aad, created_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, datetime('now')
        )
        "#,
    )
    .bind_refs(&[
        id_arg,
        conversation_arg,
        sender_actor_arg,
        sender_device_arg,
        ciphertext_arg,
        aad_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    if protocol == "mls-rfc9420" {
        persist_mls_message_metadata(
            env,
            &message_id,
            &conversation_id,
            &encrypted_message,
            &local_actor.id,
            &sender_device_id,
            &now,
        )
        .await?;
    }

    let mut note = serde_json::json!({
        "id": message_id,
        "type": "Note",
        "attributedTo": local_actor.id,
        "to": [recipient_actor_id],
        "published": now,
        "content": fallback_content,
        "daisE2ee": {
            "v": 2,
            "protocol": protocol,
            "senderDeviceId": sender_device_id,
        },
    });
    if let Some(object) = note.as_object_mut() {
        object.insert(envelope_field.to_string(), encrypted_message.clone());
        if let Some(group_id) = encrypted_message.get("groupId").cloned() {
            object
                .get_mut("daisE2ee")
                .and_then(Value::as_object_mut)
                .map(|dais| dais.insert("groupId".to_string(), group_id));
        }
        if let Some(epoch) = encrypted_message.get("epoch").cloned() {
            object
                .get_mut("daisE2ee")
                .and_then(Value::as_object_mut)
                .map(|dais| dais.insert("epoch".to_string(), epoch));
        }
    }

    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{message_id}#create"),
        "type": "Create",
        "actor": local_actor.id,
        "published": now,
        "to": [recipient_actor_id],
        "object": note
    });
    let delivery_ids = insert_delivery_rows(
        env,
        &message_id,
        vec![inbox],
        "Create",
        Some(activity.to_string()),
    )
    .await?;
    let mut message = owner_e2ee_message_by_id(env, &local_actor.id, &message_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "message not found after insert".to_string())?;
    message.insert(
        "delivery_ids".to_string(),
        Value::Array(
            delivery_ids
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        ),
    );
    let mut result = Map::new();
    result.insert("ok".to_string(), Value::Bool(true));
    result.insert("message".to_string(), Value::Object(message));
    result.insert(
        "delivery_ids".to_string(),
        Value::Array(delivery_ids.into_iter().map(Value::String).collect()),
    );
    Ok(result)
}

pub(crate) async fn owner_delete_e2ee_message(env: &Env, message_id: &str) -> Result<bool> {
    let local_actor = owner_local_actor(env).await?;
    let db = env.d1("DB")?;
    let message_arg = D1Type::Text(message_id);
    let Some(row) = db
        .prepare(
            r#"
            SELECT m.conversation_id, c.participants
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            WHERE m.id = ?1
              AND c.protocol = 'mls-rfc9420'
              AND json_valid(m.ciphertext)
              AND json_extract(m.ciphertext, '$.v') = 2
              AND json_extract(m.ciphertext, '$.protocol') = 'mls-rfc9420'
            LIMIT 1
            "#,
        )
        .bind_refs(&message_arg)?
        .first::<Map<String, Value>>(None)
        .await?
    else {
        return Ok(false);
    };
    let participants = string_vec_json_field(Some(&row), "participants");
    if !participants.iter().any(|actor| actor == &local_actor.id) {
        return Ok(false);
    }
    let conversation_id = string_field(Some(&row), "conversation_id").unwrap_or_default();

    db.prepare("DELETE FROM e2ee_mls_message_metadata WHERE message_id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;
    db.prepare("DELETE FROM e2ee_messages WHERE id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;
    db.prepare("DELETE FROM deliveries WHERE post_id = ?1")
        .bind_refs(&message_arg)?
        .run()
        .await?;

    if !conversation_id.is_empty() {
        let conversation_arg = D1Type::Text(&conversation_id);
        let remaining = db
            .prepare("SELECT id FROM e2ee_messages WHERE conversation_id = ?1 LIMIT 1")
            .bind_refs(&conversation_arg)?
            .first::<Map<String, Value>>(None)
            .await?;
        if remaining.is_none() {
            db.prepare("DELETE FROM e2ee_conversations WHERE id = ?1")
                .bind_refs(&conversation_arg)?
                .run()
                .await?;
        }
    }
    Ok(true)
}

async fn owner_e2ee_message_by_id(
    env: &Env,
    local_actor_id: &str,
    message_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let message_arg = D1Type::Text(message_id);
    let row = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT m.id, m.conversation_id, m.sender_actor_id, m.sender_device_id,
                   m.ciphertext, m.aad, m.created_at, c.participants, c.protocol
            FROM e2ee_messages m
            JOIN e2ee_conversations c ON c.id = m.conversation_id
            WHERE m.id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&message_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    match row {
        Some(row) => Ok(Some(
            owner_e2ee_message_row(env, local_actor_id, row).await?,
        )),
        None => Ok(None),
    }
}

async fn owner_e2ee_message_row(
    env: &Env,
    local_actor_id: &str,
    row: Map<String, Value>,
) -> Result<Map<String, Value>> {
    let message_id = string_field(Some(&row), "id").unwrap_or_default();
    let aad = string_field(Some(&row), "aad")
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let encrypted_message = string_field(Some(&row), "ciphertext")
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let protocol = string_field(Some(&row), "protocol")
        .and_then(|protocol| normalize_e2ee_protocol(&protocol).ok())
        .unwrap_or_else(|| "mls-rfc9420".to_string());
    let participants = string_vec_json_field(Some(&row), "participants");
    let recipient_actor_id = aad
        .get("recipientActorId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            participants
                .iter()
                .find(|actor_id| actor_id.as_str() != local_actor_id)
                .cloned()
        });
    let fallback_content = aad
        .get("fallbackContent")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let attachments = aad
        .get("attachments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let delivery_statuses = owner_delivery_rows_for_post(env, &message_id).await?;
    let mut item = Map::new();
    item.insert("id".to_string(), string_value_or_default(&row, "id"));
    item.insert(
        "conversation_id".to_string(),
        string_value_or_default(&row, "conversation_id"),
    );
    item.insert(
        "sender_actor_id".to_string(),
        string_value_or_default(&row, "sender_actor_id"),
    );
    item.insert(
        "sender_device_id".to_string(),
        string_value_or_default(&row, "sender_device_id"),
    );
    item.insert(
        "recipient_actor_id".to_string(),
        recipient_actor_id.map(Value::String).unwrap_or(Value::Null),
    );
    item.insert("e2ee_protocol".to_string(), Value::String(protocol.clone()));
    item.insert(
        "dais_encrypted_message".to_string(),
        encrypted_message.clone(),
    );
    item.insert(
        "mls_group_id".to_string(),
        encrypted_message
            .get("groupId")
            .cloned()
            .unwrap_or(Value::Null),
    );
    item.insert(
        "mls_epoch".to_string(),
        encrypted_message
            .get("epoch")
            .cloned()
            .unwrap_or(Value::Null),
    );
    item.insert(
        "fallback_content".to_string(),
        fallback_content.map(Value::String).unwrap_or(Value::Null),
    );
    item.insert("attachments".to_string(), Value::Array(attachments));
    item.insert(
        "delivery_ids".to_string(),
        Value::Array(
            delivery_statuses
                .iter()
                .filter_map(|delivery| string_field(Some(delivery), "id"))
                .map(Value::String)
                .collect(),
        ),
    );
    item.insert(
        "delivery_statuses".to_string(),
        Value::Array(delivery_statuses.into_iter().map(Value::Object).collect()),
    );
    item.insert(
        "created_at".to_string(),
        row_value_or_null(&row, "created_at"),
    );
    Ok(item)
}

pub(crate) async fn public_e2ee_devices(env: &Env, actor_id: &str) -> Result<Vec<Value>> {
    let actor_arg = D1Type::Text(actor_id);
    let rows = env
        .d1("DB")?
        .prepare(
            r#"
            SELECT device_id, display_name, protocol, credential, key_package, fingerprint, updated_at
            FROM e2ee_devices
            WHERE actor_id = ?1 AND status = 'active' AND protocol = 'mls-rfc9420'
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
            WHERE actor_id = ?1 AND protocol = 'mls-rfc9420'
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
            .unwrap_or_else(|| "mls-rfc9420".to_string())
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
            WHERE protocol = 'mls-rfc9420'
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
            .unwrap_or_else(|| "mls-rfc9420".to_string())
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
            if string_field(Some(&peer), "protocol").as_deref() != Some("mls-rfc9420") {
                return Err("recipientDeviceId must be an MLS E2EE device".to_string());
            }
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
            WHERE actor_id = ?1 AND trust_state = 'trusted' AND protocol = 'mls-rfc9420'
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
