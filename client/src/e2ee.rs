use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedMessage {
    pub v: u8,
    pub alg: String,
    #[serde(rename = "keyWrap")]
    pub key_wrap: String,
    pub iv: String,
    pub ciphertext: String,
    pub recipients: Vec<EncryptedRecipient>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedRecipient {
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "wrappedKey")]
    pub wrapped_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct EncryptedNotePayload {
    pub content: String,
    #[serde(rename = "encryptedMessage")]
    pub encrypted_message: EncryptedMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedMediaPayload {
    pub v: u8,
    pub alg: String,
    pub iv: String,
    pub ciphertext: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[allow(dead_code)]
pub fn encrypt_message(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
) -> Result<EncryptedMessage> {
    encrypt_message_with_content_key(plaintext, recipients).map(|(encrypted, _cek)| encrypted)
}

pub fn encrypt_message_with_content_key(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
) -> Result<(EncryptedMessage, String)> {
    if recipients.is_empty() {
        bail!("at least one recipient public key is required");
    }

    let mut rng = OsRng;
    let mut cek = [0u8; 32];
    rng.fill_bytes(&mut cek);

    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&cek).context("could not initialize AES-256-GCM")?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|_| anyhow!("AES-256-GCM encryption failed"))?;

    let mut wrapped = Vec::with_capacity(recipients.len());
    for (key_id, pem) in recipients {
        let public_key = RsaPublicKey::from_public_key_pem(pem)
            .with_context(|| format!("could not parse public key for {key_id}"))?;
        let wrapped_key = public_key
            .encrypt(&mut rng, Oaep::new::<Sha256>(), &cek)
            .with_context(|| format!("could not wrap CEK for {key_id}"))?;
        wrapped.push(EncryptedRecipient {
            key_id: key_id.clone(),
            wrapped_key: STANDARD.encode(wrapped_key),
        });
    }

    let content_key = STANDARD.encode(cek);
    let encrypted = EncryptedMessage {
        v: 1,
        alg: "AES-256-GCM".to_string(),
        key_wrap: "RSA-OAEP-256".to_string(),
        iv: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(ciphertext),
        recipients: wrapped,
    };

    Ok((encrypted, content_key))
}

pub fn decrypt_message(
    encrypted: &EncryptedMessage,
    private_key_pem: &str,
    my_key_id: Option<&str>,
) -> Result<String> {
    let (plaintext, _content_key) =
        decrypt_message_with_content_key(encrypted, private_key_pem, my_key_id)?;
    Ok(plaintext)
}

pub fn decrypt_message_with_content_key(
    encrypted: &EncryptedMessage,
    private_key_pem: &str,
    my_key_id: Option<&str>,
) -> Result<(String, String)> {
    validate_envelope(encrypted)?;
    let recipient = select_recipient(encrypted, my_key_id)?;
    let private_key =
        RsaPrivateKey::from_pkcs8_pem(private_key_pem).context("could not parse private key")?;
    let cek = private_key
        .decrypt(
            Oaep::new::<Sha256>(),
            &STANDARD
                .decode(&recipient.wrapped_key)
                .context("wrappedKey is not valid base64")?,
        )
        .context("could not unwrap content key")?;

    let iv = STANDARD
        .decode(&encrypted.iv)
        .context("iv is not valid base64")?;
    let ciphertext = STANDARD
        .decode(&encrypted.ciphertext)
        .context("ciphertext is not valid base64")?;
    let cipher = Aes256Gcm::new_from_slice(&cek).context("could not initialize AES-256-GCM")?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&iv), ciphertext.as_ref())
        .map_err(|_| anyhow!("AES-256-GCM decryption failed"))?;

    let plaintext = String::from_utf8(plaintext).context("plaintext was not valid UTF-8")?;
    Ok((plaintext, STANDARD.encode(cek)))
}

pub fn encrypt_media_bytes_with_content_key(
    bytes: &[u8],
    content_key: &str,
    media_type: &str,
    name: Option<&str>,
) -> Result<EncryptedMediaPayload> {
    let cek = STANDARD
        .decode(content_key)
        .context("content key is not valid base64")?;
    if cek.len() != 32 {
        bail!("content key must be 32 bytes for AES-256-GCM");
    }

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&cek).context("could not initialize AES-256-GCM")?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), bytes)
        .map_err(|_| anyhow!("AES-256-GCM media encryption failed"))?;

    Ok(EncryptedMediaPayload {
        v: 1,
        alg: "AES-256-GCM".to_string(),
        iv: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(ciphertext),
        media_type: media_type.to_string(),
        name: name.map(ToString::to_string),
    })
}

#[allow(dead_code)]
pub fn decrypt_media_bytes_with_content_key(
    encrypted: &EncryptedMediaPayload,
    content_key: &str,
) -> Result<Vec<u8>> {
    validate_media_envelope(encrypted)?;
    let cek = STANDARD
        .decode(content_key)
        .context("content key is not valid base64")?;
    if cek.len() != 32 {
        bail!("content key must be 32 bytes for AES-256-GCM");
    }
    let iv = STANDARD
        .decode(&encrypted.iv)
        .context("media iv is not valid base64")?;
    let ciphertext = STANDARD
        .decode(&encrypted.ciphertext)
        .context("media ciphertext is not valid base64")?;
    let cipher = Aes256Gcm::new_from_slice(&cek).context("could not initialize AES-256-GCM")?;
    cipher
        .decrypt(Nonce::from_slice(&iv), ciphertext.as_ref())
        .map_err(|_| anyhow!("AES-256-GCM media decryption failed"))
}

pub fn encrypted_note_payload(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
    view_url: Option<&str>,
) -> Result<EncryptedNotePayload> {
    encrypted_note_payload_with_content_key(plaintext, recipients, view_url)
        .map(|(payload, _cek)| payload)
}

pub fn encrypted_note_payload_with_content_key(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
    view_url: Option<&str>,
) -> Result<(EncryptedNotePayload, String)> {
    let (encrypted_message, content_key) = encrypt_message_with_content_key(plaintext, recipients)?;
    let payload = EncryptedNotePayload {
        content: fallback_content(view_url),
        encrypted_message,
    };

    Ok((payload, content_key))
}

pub fn fallback_content(view_url: Option<&str>) -> String {
    let link = view_url
        .map(|url| format!("To read it, open it in dais: <a href=\"{url}\">{url}</a><br>"))
        .unwrap_or_default();
    format!(
        "🔒 <strong>End-to-end encrypted message</strong><br>\
This message was sent encrypted, so your current client can’t display it.<br>\
{link}<em>You’ll need a dais-compatible client to read it — learn more at \
<a href=\"https://dais.social\">dais.social</a>.</em>"
    )
}

pub fn encrypted_message_from_json(value: Value) -> Result<EncryptedMessage> {
    let encrypted = value.get("encryptedMessage").cloned().unwrap_or(value);
    serde_json::from_value(encrypted).context("could not decode encryptedMessage")
}

pub fn encrypted_media_from_json(value: Value) -> Result<EncryptedMediaPayload> {
    let encrypted = value.get("encryptedMedia").cloned().unwrap_or(value);
    serde_json::from_value(encrypted).context("could not decode encryptedMedia")
}

fn validate_envelope(encrypted: &EncryptedMessage) -> Result<()> {
    if encrypted.v != 1 {
        bail!("unsupported encryptedMessage version {}", encrypted.v);
    }
    if encrypted.alg != "AES-256-GCM" {
        bail!("unsupported content cipher {}", encrypted.alg);
    }
    if encrypted.key_wrap != "RSA-OAEP-256" {
        bail!("unsupported key wrap {}", encrypted.key_wrap);
    }
    Ok(())
}

fn validate_media_envelope(encrypted: &EncryptedMediaPayload) -> Result<()> {
    if encrypted.v != 1 {
        bail!("unsupported encryptedMedia version {}", encrypted.v);
    }
    if encrypted.alg != "AES-256-GCM" {
        bail!("unsupported media cipher {}", encrypted.alg);
    }
    Ok(())
}

fn select_recipient<'a>(
    encrypted: &'a EncryptedMessage,
    my_key_id: Option<&str>,
) -> Result<&'a EncryptedRecipient> {
    if let Some(my_key_id) = my_key_id {
        if let Some(recipient) = encrypted
            .recipients
            .iter()
            .find(|recipient| recipient.key_id == my_key_id)
        {
            return Ok(recipient);
        }
    }

    if encrypted.recipients.len() == 1 {
        return Ok(&encrypted.recipients[0]);
    }

    bail!("no matching recipient key for this message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    fn test_keypair() -> (String, String) {
        let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let public = RsaPublicKey::from(&private);
        (
            private.to_pkcs8_pem(LineEnding::LF).unwrap().to_string(),
            public.to_public_key_pem(LineEnding::LF).unwrap(),
        )
    }

    #[test]
    fn round_trips_message() {
        let (private_pem, public_pem) = test_keypair();
        let key_id = "https://alice.example/actor#main-key";
        let recipients = BTreeMap::from([(key_id.to_string(), public_pem)]);
        let encrypted = encrypt_message("secret message", &recipients).unwrap();

        let decrypted = decrypt_message(&encrypted, &private_pem, Some(key_id)).unwrap();

        assert_eq!(decrypted, "secret message");
    }

    #[test]
    fn serialized_envelope_does_not_contain_plaintext() {
        let (_private_pem, public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let encrypted = encrypt_message("plaintext must not appear", &recipients).unwrap();
        let serialized = serde_json::to_string(&encrypted).unwrap();

        assert!(!serialized.contains("plaintext must not appear"));
    }

    #[test]
    fn wrong_recipient_key_fails() {
        let (_private_pem, public_pem) = test_keypair();
        let (wrong_private_pem, _wrong_public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let encrypted = encrypt_message("secret", &recipients).unwrap();

        assert!(decrypt_message(&encrypted, &wrong_private_pem, None).is_err());
    }

    #[test]
    fn fallback_contains_notice_and_optional_link() {
        let fallback = fallback_content(Some("https://dais.social/messages/1"));

        assert!(fallback.contains("End-to-end encrypted message"));
        assert!(fallback.contains("https://dais.social/messages/1"));
    }

    #[test]
    fn content_key_can_decrypt_hosted_read_fragment_payload() {
        let (_private_pem, public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let (encrypted, content_key) =
            encrypt_message_with_content_key("link key decrypts", &recipients).unwrap();
        let cek = STANDARD.decode(content_key).unwrap();
        let iv = STANDARD.decode(encrypted.iv).unwrap();
        let ciphertext = STANDARD.decode(encrypted.ciphertext).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&cek).unwrap();
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&iv), ciphertext.as_ref())
            .unwrap();

        assert_eq!(String::from_utf8(plaintext).unwrap(), "link key decrypts");
    }

    #[test]
    fn keyless_fallback_does_not_contain_content_key() {
        let (_private_pem, public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let (payload, content_key) = encrypted_note_payload_with_content_key(
            "strict fallback",
            &recipients,
            Some("https://dais.social/messages/1"),
        )
        .unwrap();

        assert!(!payload.content.contains(&content_key));
        assert!(!payload.content.contains("#cek="));
    }

    #[test]
    fn content_key_round_trips_media_bytes() {
        let (_private_pem, public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let (_message, content_key) =
            encrypt_message_with_content_key("message with media", &recipients).unwrap();
        let encrypted = encrypt_media_bytes_with_content_key(
            b"private image bytes",
            &content_key,
            "image/png",
            Some("image.png"),
        )
        .unwrap();

        let decrypted = decrypt_media_bytes_with_content_key(&encrypted, &content_key).unwrap();

        assert_eq!(decrypted, b"private image bytes");
        assert_eq!(encrypted.media_type, "image/png");
        assert_eq!(encrypted.name.as_deref(), Some("image.png"));
    }

    #[test]
    fn serialized_media_envelope_does_not_contain_plaintext() {
        let (_private_pem, public_pem) = test_keypair();
        let recipients = BTreeMap::from([(
            "https://alice.example/actor#main-key".to_string(),
            public_pem,
        )]);
        let (_message, content_key) =
            encrypt_message_with_content_key("message with media", &recipients).unwrap();
        let encrypted = encrypt_media_bytes_with_content_key(
            b"do not leak media",
            &content_key,
            "application/octet-stream",
            None,
        )
        .unwrap();
        let serialized = serde_json::to_string(&encrypted).unwrap();

        assert!(!serialized.contains("do not leak media"));
    }
}
