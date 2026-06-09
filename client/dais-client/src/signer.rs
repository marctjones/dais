//! Request signing — the SDK is the only place that touches your private key (§3).
//!
//! Thin wrapper over `dais_shared::signatures` so the CLI/TUI never re-implement
//! signing. Signing logic itself is the same audited code the server uses.

use std::collections::HashMap;

use dais_shared::signatures::{self, HttpSignature};

use crate::config::Config;
use crate::error::{Error, Result};

/// Holds the loaded private key + keyId and produces HTTP Signatures.
pub struct Signer {
    private_key_pem: String,
    key_id: String,
}

impl Signer {
    pub fn new(private_key_pem: String, key_id: String) -> Self {
        Signer {
            private_key_pem,
            key_id,
        }
    }

    /// Build a signer from config (reads the key file, requires `keys.key_id`).
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let key_id = cfg
            .keys
            .key_id
            .clone()
            .ok_or_else(|| Error::NotConfigured("keys.key_id".into()))?;
        let pem = cfg.read_private_key()?;
        Ok(Signer::new(pem, key_id))
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Sign an outbound HTTP request, returning the `Signature` header value.
    pub fn sign(
        &self,
        method: &str,
        path: &str,
        headers: &HashMap<String, String>,
        headers_to_sign: &[String],
    ) -> Result<HttpSignature> {
        signatures::sign_request(
            &self.private_key_pem,
            &self.key_id,
            method,
            path,
            headers,
            headers_to_sign,
        )
        .map_err(Error::Crypto)
    }
}
