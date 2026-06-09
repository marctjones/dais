//! End-to-end encryption — re-exported from `dais_shared` so the client and the
//! server share one audited implementation (CLIENT_REDESIGN.md §2/§3).
//!
//! Encryption happens here, client-side, before anything leaves the machine.

pub use dais_shared::e2ee::{
    decrypt_message, encrypt_message, fallback_content, EncryptedMessage, WrappedKey,
};

use crate::config::Config;
use crate::error::{Error, Result};

/// Encrypt `plaintext` to yourself using the configured key (v1 reuses the actor
/// RSA key — #71). Returns the `encryptedMessage` extension plus the fallback notice
/// that non-dais clients will display.
pub fn encrypt_to_self(
    cfg: &Config,
    plaintext: &str,
    view_url: Option<&str>,
) -> Result<(EncryptedMessage, String)> {
    let key_id = cfg
        .keys
        .key_id
        .clone()
        .ok_or_else(|| Error::NotConfigured("keys.key_id".into()))?;
    let public_pem = public_key_from_private(&cfg.read_private_key()?)?;
    let enc = encrypt_message(plaintext, &[(key_id, public_pem)]).map_err(Error::Crypto)?;
    Ok((enc, fallback_content(view_url)))
}

/// Decrypt an `encryptedMessage` with the configured private key.
pub fn decrypt_with_config(cfg: &Config, enc: &EncryptedMessage) -> Result<String> {
    let pem = cfg.read_private_key()?;
    decrypt_message(enc, &pem, cfg.keys.key_id.as_deref()).map_err(Error::Crypto)
}

/// Derive the SPKI public-key PEM from a PKCS#8 private-key PEM.
fn public_key_from_private(private_pem: &str) -> Result<String> {
    use dais_shared::rsa::pkcs8::{DecodePrivateKey, EncodePublicKey, LineEnding};
    use dais_shared::rsa::{RsaPrivateKey, RsaPublicKey};

    let priv_key = RsaPrivateKey::from_pkcs8_pem(private_pem)
        .map_err(|e| Error::Crypto(format!("parsing private key: {e}")))?;
    RsaPublicKey::from(&priv_key)
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| Error::Crypto(format!("encoding public key: {e}")))
}
