//! Client configuration with precedence: env vars > user config file > defaults
//! (CLIENT_REDESIGN.md P4). The CLI applies flag overrides on top of what's loaded
//! here, completing `flags > env > file > defaults`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::Visibility;

/// Top-level config, persisted as TOML at `~/.config/dais/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Your full handle, e.g. `@social@dais.social`.
    #[serde(default)]
    pub handle: Option<String>,
    /// Your instance domain, e.g. `dais.social`.
    #[serde(default)]
    pub instance: Option<String>,

    #[serde(default)]
    pub d1: D1Config,
    #[serde(default)]
    pub keys: KeyConfig,
    #[serde(default)]
    pub defaults: Defaults,
}

/// Cloudflare D1 HTTP API credentials (kills `wrangler d1 execute` shelling).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct D1Config {
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub database_id: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
}

impl D1Config {
    pub fn is_complete(&self) -> bool {
        self.account_id.is_some() && self.database_id.is_some() && self.api_token.is_some()
    }
}

/// Local signing key (the SDK is the only thing that touches secrets — §3).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyConfig {
    /// Path to the PKCS#8 PEM private key (e.g. `~/.dais/keys/private.pem`).
    #[serde(default)]
    pub private_key_path: Option<String>,
    /// keyId published in your actor (e.g. `https://dais.social/users/social#main-key`).
    #[serde(default)]
    pub key_id: Option<String>,
}

/// Composer defaults — private by default (#62).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub encrypt: bool,
}

fn default_visibility() -> String {
    "followers".to_string()
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            visibility: default_visibility(),
            encrypt: false,
        }
    }
}

impl Config {
    /// Load config from the user file (if any), then overlay environment variables.
    pub fn load() -> Result<Self> {
        let mut cfg = match std::fs::read_to_string(Self::config_path()?) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| Error::Config(format!("parsing config.toml: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(Error::Io(e)),
        };
        cfg.apply_env();
        Ok(cfg)
    }

    /// Overlay `DAIS_*` environment variables (higher precedence than the file).
    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("DAIS_HANDLE") {
            self.handle = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_INSTANCE") {
            self.instance = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_D1_ACCOUNT_ID") {
            self.d1.account_id = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_D1_DATABASE_ID") {
            self.d1.database_id = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_D1_API_TOKEN") {
            self.d1.api_token = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_PRIVATE_KEY_PATH") {
            self.keys.private_key_path = Some(v);
        }
        if let Ok(v) = std::env::var("DAIS_KEY_ID") {
            self.keys.key_id = Some(v);
        }
    }

    /// Persist to the user config file (creating the directory if needed).
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("serializing config: {e}")))?;
        std::fs::write(&path, text)?;
        Ok(())
    }

    pub fn default_visibility(&self) -> Visibility {
        Visibility::parse(&self.defaults.visibility).unwrap_or(Visibility::Followers)
    }

    pub fn require_handle(&self) -> Result<&str> {
        self.handle
            .as_deref()
            .ok_or_else(|| Error::NotConfigured("handle".into()))
    }

    pub fn require_instance(&self) -> Result<&str> {
        self.instance
            .as_deref()
            .ok_or_else(|| Error::NotConfigured("instance".into()))
    }

    /// `~/.config/dais/config.toml` (honors `DAIS_CONFIG`).
    pub fn config_path() -> Result<PathBuf> {
        if let Ok(p) = std::env::var("DAIS_CONFIG") {
            return Ok(PathBuf::from(p));
        }
        let dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("cannot resolve config dir".into()))?;
        Ok(dir.join("dais").join("config.toml"))
    }

    /// `~/.local/share/dais/store.db` (honors `DAIS_STORE`).
    pub fn store_path() -> Result<PathBuf> {
        if let Ok(p) = std::env::var("DAIS_STORE") {
            return Ok(PathBuf::from(p));
        }
        let dir = dirs::data_dir()
            .ok_or_else(|| Error::Config("cannot resolve data dir".into()))?;
        Ok(dir.join("dais").join("store.db"))
    }

    /// Expand a leading `~/` in a configured path.
    pub fn expand_path(p: &str) -> PathBuf {
        if let Some(rest) = p.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(p)
    }

    /// Read the configured private key PEM, if set and present.
    pub fn read_private_key(&self) -> Result<String> {
        let path = self
            .keys
            .private_key_path
            .as_deref()
            .ok_or_else(|| Error::NotConfigured("keys.private_key_path".into()))?;
        let path: &Path = &Self::expand_path(path);
        std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("reading private key {}: {e}", path.display())))
    }
}
