use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlueskyConfig {
    pub handle: String,
    pub did: String,
    pub password: String,
    pub service: String,
    pub appview: String,
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    root: PathBuf,
}

impl ConfigStore {
    pub fn default() -> Result<Self> {
        let root = if let Some(path) = std::env::var_os("DAIS_HOME") {
            PathBuf::from(path)
        } else {
            std::env::current_dir()?.join(".dais")
        };

        Ok(Self { root })
    }

    #[cfg(test)]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn load_bluesky(&self) -> Result<BlueskyConfig> {
        let path = self.bluesky_path();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Bluesky config not found at {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("invalid Bluesky config at {}", path.display()))
    }

    pub fn save_bluesky(&self, config: &BlueskyConfig) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.bluesky_path();
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&path, content)?;
        set_owner_read_write(&path)?;
        Ok(())
    }

    pub fn delete_bluesky(&self) -> Result<()> {
        let path = self.bluesky_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn bluesky_path(&self) -> PathBuf {
        self.root.join("bluesky.json")
    }
}

#[cfg(unix)]
fn set_owner_read_write(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_read_write(_path: &PathBuf) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BlueskyConfig, ConfigStore};

    #[test]
    fn round_trips_bluesky_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        let config = BlueskyConfig {
            handle: "alice.bsky.social".to_string(),
            did: "did:plc:alice".to_string(),
            password: "app-password".to_string(),
            service: "https://bsky.social".to_string(),
            appview: "https://api.bsky.app".to_string(),
        };

        store.save_bluesky(&config).unwrap();
        let loaded = store.load_bluesky().unwrap();

        assert_eq!(loaded.handle, config.handle);
        assert_eq!(loaded.did, config.did);
        assert_eq!(loaded.service, config.service);
        assert_eq!(loaded.appview, config.appview);
    }
}
