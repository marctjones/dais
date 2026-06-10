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

pub fn encrypt_message(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
) -> Result<EncryptedMessage> {
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

    Ok(EncryptedMessage {
        v: 1,
        alg: "AES-256-GCM".to_string(),
        key_wrap: "RSA-OAEP-256".to_string(),
        iv: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(ciphertext),
        recipients: wrapped,
    })
}

pub fn decrypt_message(
    encrypted: &EncryptedMessage,
    private_key_pem: &str,
    my_key_id: Option<&str>,
) -> Result<String> {
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

    String::from_utf8(plaintext).context("plaintext was not valid UTF-8")
}

pub fn encrypted_note_payload(
    plaintext: &str,
    recipients: &BTreeMap<String, String>,
    view_url: Option<&str>,
) -> Result<EncryptedNotePayload> {
    Ok(EncryptedNotePayload {
        content: fallback_content(view_url),
        encrypted_message: encrypt_message(plaintext, recipients)?,
    })
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
}
