//! End-to-end encryption for dais messages, with graceful fallback for clients
//! that don't support it.
//!
//! This is a Rust port of `cli/dais_cli/e2ee.py`, kept **wire-compatible** so a
//! message encrypted by the Python client decrypts here (and vice-versa) and both
//! interoperate over the fediverse.
//!
//! ## Wire format (the part that matters for interop)
//! The ActivityPub Note's `content` field carries a human-readable FALLBACK NOTICE
//! ([`fallback_content`]), so non-supporting clients (Mastodon, etc.) show that an
//! encrypted message arrived and how to read it — never silent gibberish. The actual
//! ciphertext travels in an `encryptedMessage` extension ([`EncryptedMessage`]) that
//! dais clients understand and others harmlessly ignore.
//!
//! ## Crypto (v1)
//! Hybrid: AES-256-GCM content encryption + RSA-OAEP(SHA-256) key wrapping to each
//! recipient's published RSA public key. v1 reuses the actor RSA key — a pragmatic
//! first cut, flagged for replacement by MLS (RFC 9420) per #71.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand_core::{OsRng, RngCore};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// The `encryptedMessage` extension structure carried on an ActivityPub Note.
///
/// Field names and values mirror the Python implementation exactly so the JSON is
/// byte-for-byte interoperable: `{"v":1,"alg":"AES-256-GCM","keyWrap":"RSA-OAEP-256",
/// "iv":...,"ciphertext":...,"recipients":[{"keyId":...,"wrappedKey":...}]}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMessage {
    pub v: u32,
    pub alg: String,
    #[serde(rename = "keyWrap")]
    pub key_wrap: String,
    /// base64(12-byte AES-GCM nonce)
    pub iv: String,
    /// base64(AES-GCM ciphertext, with the 16-byte tag appended — matches Python's
    /// `cryptography` AESGCM output)
    pub ciphertext: String,
    pub recipients: Vec<WrappedKey>,
}

/// One recipient's wrapped content-encryption key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    #[serde(rename = "keyId")]
    pub key_id: String,
    /// base64(RSA-OAEP-SHA256 wrapped CEK)
    #[serde(rename = "wrappedKey")]
    pub wrapped_key: String,
}

/// Encrypt `plaintext` for one or more recipients.
///
/// `recipients` is a list of `(key_id, public_key_pem)` pairs — one per recipient.
/// Returns the [`EncryptedMessage`] extension structure.
pub fn encrypt_message(
    plaintext: &str,
    recipients: &[(String, String)],
) -> Result<EncryptedMessage, String> {
    if recipients.is_empty() {
        return Err("at least one recipient public key is required".to_string());
    }

    // 1. Encrypt the content once with a fresh 256-bit symmetric key.
    let mut cek = [0u8; 32];
    OsRng.fill_bytes(&mut cek);
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(&cek)
        .map_err(|e| format!("Failed to init AES-256-GCM: {}", e))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| format!("AES-GCM encryption failed: {}", e))?;

    // 2. Wrap the content key to each recipient's RSA public key (OAEP-SHA256).
    let mut wrapped = Vec::with_capacity(recipients.len());
    for (key_id, pem) in recipients {
        let pub_key = RsaPublicKey::from_public_key_pem(pem)
            .map_err(|e| format!("Failed to parse recipient public key: {}", e))?;
        let wk = pub_key
            .encrypt(&mut OsRng, Oaep::new::<Sha256>(), &cek)
            .map_err(|e| format!("Key wrap failed: {}", e))?;
        wrapped.push(WrappedKey {
            key_id: key_id.clone(),
            wrapped_key: BASE64.encode(wk),
        });
    }

    Ok(EncryptedMessage {
        v: 1,
        alg: "AES-256-GCM".to_string(),
        key_wrap: "RSA-OAEP-256".to_string(),
        iv: BASE64.encode(nonce_bytes),
        ciphertext: BASE64.encode(ciphertext),
        recipients: wrapped,
    })
}

/// Decrypt an [`EncryptedMessage`] with our private key.
///
/// If `my_key_id` is given, the matching recipient entry is used; otherwise, when
/// there is exactly one recipient, that entry is used (mirrors the Python behavior).
pub fn decrypt_message(
    enc: &EncryptedMessage,
    private_key_pem: &str,
    my_key_id: Option<&str>,
) -> Result<String, String> {
    let mine = match my_key_id {
        Some(id) => enc.recipients.iter().find(|r| r.key_id == id),
        None => None,
    }
    .or_else(|| {
        if enc.recipients.len() == 1 {
            enc.recipients.first()
        } else {
            None
        }
    })
    .ok_or("no matching recipient key for this message")?;

    let priv_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| format!("Failed to parse private key: {}", e))?;

    let wrapped = BASE64
        .decode(&mine.wrapped_key)
        .map_err(|e| format!("Failed to decode wrapped key: {}", e))?;
    let cek = priv_key
        .decrypt(Oaep::new::<Sha256>(), &wrapped)
        .map_err(|e| format!("Key unwrap failed: {}", e))?;

    let nonce = BASE64
        .decode(&enc.iv)
        .map_err(|e| format!("Failed to decode iv: {}", e))?;
    let ciphertext = BASE64
        .decode(&enc.ciphertext)
        .map_err(|e| format!("Failed to decode ciphertext: {}", e))?;

    let cipher = Aes256Gcm::new_from_slice(&cek)
        .map_err(|e| format!("Failed to init AES-256-GCM: {}", e))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
        .map_err(|e| format!("AES-GCM decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 plaintext: {}", e))
}

/// The notice rendered by clients that can't decrypt (e.g. Mastodon).
///
/// This is what makes E2EE graceful over the fediverse: instead of gibberish, the
/// recipient sees that they got an encrypted message and how to read it. Kept
/// identical to the Python `fallback_content` so the notice is consistent regardless
/// of which client sent it.
pub fn fallback_content(view_url: Option<&str>) -> String {
    let link = match view_url {
        Some(url) => format!(
            "To read it, open it in dais: <a href=\"{url}\">{url}</a><br>"
        ),
        None => String::new(),
    };
    format!(
        "🔒 <strong>End-to-end encrypted message</strong><br>\
         This message was sent encrypted, so your current client can’t display it.<br>\
         {link}\
         <em>You’ll need a dais-compatible client to read it — learn more at \
         <a href=\"https://dais.social\">dais.social</a>.</em>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    fn test_keypair() -> (String, String) {
        let priv_key = RsaPrivateKey::new(&mut OsRng, 2048).expect("keygen");
        let pub_key = RsaPublicKey::from(&priv_key);
        let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
        let pub_pem = pub_key.to_public_key_pem(LineEnding::LF).unwrap();
        (priv_pem, pub_pem)
    }

    #[test]
    fn round_trip_single_recipient() {
        let (priv_pem, pub_pem) = test_keypair();
        let key_id = "https://dais.social/users/me#main-key".to_string();

        let enc = encrypt_message("hello, friends 🔒", &[(key_id.clone(), pub_pem)]).unwrap();

        assert_eq!(enc.v, 1);
        assert_eq!(enc.alg, "AES-256-GCM");
        assert_eq!(enc.key_wrap, "RSA-OAEP-256");
        assert_eq!(enc.recipients.len(), 1);

        // Decrypt by explicit key id, and via the single-recipient fallback path.
        assert_eq!(
            decrypt_message(&enc, &priv_pem, Some(&key_id)).unwrap(),
            "hello, friends 🔒"
        );
        assert_eq!(
            decrypt_message(&enc, &priv_pem, None).unwrap(),
            "hello, friends 🔒"
        );
    }

    #[test]
    fn wire_shape_matches_python() {
        // Serialized JSON must use the exact keys the Python side emits/reads.
        let (_priv, pub_pem) = test_keypair();
        let enc = encrypt_message("x", &[("k".to_string(), pub_pem)]).unwrap();
        let json = serde_json::to_value(&enc).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["alg"], "AES-256-GCM");
        assert_eq!(json["keyWrap"], "RSA-OAEP-256");
        assert!(json["iv"].is_string());
        assert!(json["ciphertext"].is_string());
        assert_eq!(json["recipients"][0]["keyId"], "k");
        assert!(json["recipients"][0]["wrappedKey"].is_string());
    }

    #[test]
    fn fallback_includes_link_when_url_given() {
        let with = fallback_content(Some("https://dais.social/p/1"));
        assert!(with.contains("https://dais.social/p/1"));
        let without = fallback_content(None);
        assert!(!without.contains("open it in dais"));
        assert!(without.contains("End-to-end encrypted message"));
    }
}
