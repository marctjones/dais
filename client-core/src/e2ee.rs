use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
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

pub type E2eeResult<T> = Result<T, String>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptedMessage {
    pub v: u8,
    pub alg: String,
    #[serde(rename = "keyWrap")]
    pub key_wrap: String,
    pub iv: String,
    pub ciphertext: String,
    pub recipients: Vec<EncryptedRecipient>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncryptedRecipient {
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "wrappedKey")]
    pub wrapped_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EncryptedNotePayload {
    pub content: String,
    #[serde(rename = "encryptedMessage")]
    pub encrypted_message: EncryptedMessage,
}

pub fn encrypt_message(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
) -> E2eeResult<EncryptedMessage> {
    encrypt_message_with_content_key(plaintext, recipients).map(|(encrypted, _cek)| encrypted)
}

pub fn encrypt_message_with_content_key(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
) -> E2eeResult<(EncryptedMessage, String)> {
    if recipients.is_empty() {
        return Err("at least one recipient public key is required".to_string());
    }

    let mut rng = OsRng;
    let mut cek = [0u8; 32];
    rng.fill_bytes(&mut cek);

    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&cek)
        .map_err(|_| "could not initialize AES-256-GCM".to_string())?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|_| "AES-256-GCM encryption failed".to_string())?;

    let mut wrapped = Vec::with_capacity(recipients.len());
    for (key_id, pem) in recipients {
        let public_key = RsaPublicKey::from_public_key_pem(pem)
            .map_err(|error| format!("could not parse public key for {key_id}: {error}"))?;
        let wrapped_key = public_key
            .encrypt(&mut rng, Oaep::new::<Sha256>(), &cek)
            .map_err(|error| format!("could not wrap CEK for {key_id}: {error}"))?;
        wrapped.push(EncryptedRecipient {
            key_id: key_id.clone(),
            wrapped_key: STANDARD.encode(wrapped_key),
        });
    }

    let encrypted = EncryptedMessage {
        v: 1,
        alg: "AES-256-GCM".to_string(),
        key_wrap: "RSA-OAEP-256".to_string(),
        iv: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(ciphertext),
        recipients: wrapped,
    };

    Ok((encrypted, STANDARD.encode(cek)))
}

pub fn decrypt_message(
    encrypted: &EncryptedMessage,
    private_key_pem: &str,
    my_key_id: Option<&str>,
) -> E2eeResult<String> {
    validate_envelope(encrypted)?;
    let recipient = select_recipient(encrypted, my_key_id)?;
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|error| format!("could not parse private key: {error}"))?;
    let wrapped_key = STANDARD
        .decode(&recipient.wrapped_key)
        .map_err(|error| format!("wrappedKey is not valid base64: {error}"))?;
    let cek = private_key
        .decrypt(Oaep::new::<Sha256>(), &wrapped_key)
        .map_err(|error| format!("could not unwrap content key: {error}"))?;

    let iv = STANDARD
        .decode(&encrypted.iv)
        .map_err(|error| format!("iv is not valid base64: {error}"))?;
    let ciphertext = STANDARD
        .decode(&encrypted.ciphertext)
        .map_err(|error| format!("ciphertext is not valid base64: {error}"))?;
    let cipher = Aes256Gcm::new_from_slice(&cek)
        .map_err(|_| "could not initialize AES-256-GCM".to_string())?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&iv), ciphertext.as_ref())
        .map_err(|_| "AES-256-GCM decryption failed".to_string())?;

    String::from_utf8(plaintext).map_err(|error| format!("plaintext was not valid UTF-8: {error}"))
}

pub fn encrypted_note_payload(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
    view_url: Option<&str>,
) -> E2eeResult<EncryptedNotePayload> {
    encrypted_note_payload_with_content_key(plaintext, recipients, view_url)
        .map(|(payload, _cek)| payload)
}

pub fn encrypted_note_payload_with_content_key(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
    view_url: Option<&str>,
) -> E2eeResult<(EncryptedNotePayload, String)> {
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

pub fn encrypted_message_from_json(value: Value) -> E2eeResult<EncryptedMessage> {
    let encrypted = value.get("encryptedMessage").cloned().unwrap_or(value);
    serde_json::from_value(encrypted)
        .map_err(|error| format!("could not decode encryptedMessage: {error}"))
}

pub fn validate_envelope(encrypted: &EncryptedMessage) -> E2eeResult<()> {
    if encrypted.v != 1 {
        return Err(format!(
            "unsupported encryptedMessage version {}",
            encrypted.v
        ));
    }
    if encrypted.alg != "AES-256-GCM" {
        return Err(format!("unsupported content cipher {}", encrypted.alg));
    }
    if encrypted.key_wrap != "RSA-OAEP-256" {
        return Err(format!("unsupported key wrap {}", encrypted.key_wrap));
    }
    if encrypted.recipients.is_empty() {
        return Err("encryptedMessage must include at least one recipient".to_string());
    }
    STANDARD
        .decode(&encrypted.iv)
        .map_err(|error| format!("iv is not valid base64: {error}"))?;
    STANDARD
        .decode(&encrypted.ciphertext)
        .map_err(|error| format!("ciphertext is not valid base64: {error}"))?;
    for recipient in &encrypted.recipients {
        if recipient.key_id.trim().is_empty() {
            return Err("recipient keyId is required".to_string());
        }
        STANDARD
            .decode(&recipient.wrapped_key)
            .map_err(|error| format!("wrappedKey is not valid base64: {error}"))?;
    }
    Ok(())
}

fn select_recipient<'a>(
    encrypted: &'a EncryptedMessage,
    my_key_id: Option<&str>,
) -> E2eeResult<&'a EncryptedRecipient> {
    if let Some(my_key_id) = my_key_id {
        if let Some(recipient) = encrypted.recipients.iter().find(|recipient| {
            recipient.key_id == my_key_id || recipient.key_id.ends_with(&format!("#{my_key_id}"))
        }) {
            return Ok(recipient);
        }
    }

    if encrypted.recipients.len() == 1 {
        return Ok(&encrypted.recipients[0]);
    }

    Err("no matching recipient key for this message".to_string())
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
    fn grouped_key_id_can_be_selected_by_device_suffix() {
        let (_alice_private, alice_public) = test_keypair();
        let (bob_private, bob_public) = test_keypair();
        let recipients = BTreeMap::from([
            (
                "https://alice.example/users/alice#phone".to_string(),
                alice_public,
            ),
            (
                "https://bob.example/users/bob#laptop".to_string(),
                bob_public,
            ),
        ]);
        let encrypted = encrypt_message("group secret", &recipients).unwrap();

        let decrypted = decrypt_message(&encrypted, &bob_private, Some("laptop")).unwrap();

        assert_eq!(decrypted, "group secret");
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

        assert!(payload.content.contains("End-to-end encrypted message"));
        assert!(!payload.content.contains(&content_key));
        assert!(!payload.content.contains("#cek="));
    }

    #[test]
    fn invalid_envelope_is_rejected() {
        let encrypted = EncryptedMessage {
            v: 9,
            alg: "AES-256-GCM".to_string(),
            key_wrap: "RSA-OAEP-256".to_string(),
            iv: STANDARD.encode([0u8; 12]),
            ciphertext: STANDARD.encode([1u8; 32]),
            recipients: vec![EncryptedRecipient {
                key_id: "key".to_string(),
                wrapped_key: STANDARD.encode([2u8; 32]),
            }],
        };

        assert!(validate_envelope(&encrypted).is_err());
    }
}
