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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct E2eePrivateKeyEntry {
    pub instance: String,
    pub device_id: String,
    pub path: PathBuf,
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

    pub fn e2ee_private_key_path(&self, instance_url: &str, device_id: &str) -> PathBuf {
        self.root
            .join("e2ee")
            .join(safe_path_component(instance_url))
            .join(format!("{}.pkcs8.pem", safe_path_component(device_id)))
    }

    pub fn save_e2ee_private_key(
        &self,
        instance_url: &str,
        device_id: &str,
        private_key_pem: &str,
        force: bool,
    ) -> Result<PathBuf> {
        let path = self.e2ee_private_key_path(instance_url, device_id);
        if path.exists() && !force {
            anyhow::bail!(
                "{} already exists; pass --force to overwrite",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, private_key_pem.as_bytes())?;
        set_owner_read_write(&path)?;
        Ok(path)
    }

    pub fn load_e2ee_private_key(&self, instance_url: &str, device_id: &str) -> Result<String> {
        let path = self.e2ee_private_key_path(instance_url, device_id);
        fs::read_to_string(&path)
            .with_context(|| format!("E2EE private key not found at {}", path.display()))
    }

    pub fn list_e2ee_private_keys(&self) -> Result<Vec<E2eePrivateKeyEntry>> {
        let root = self.root.join("e2ee");
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for instance_entry in fs::read_dir(root)? {
            let instance_entry = instance_entry?;
            if !instance_entry.file_type()?.is_dir() {
                continue;
            }
            let instance = instance_entry.file_name().to_string_lossy().to_string();
            for key_entry in fs::read_dir(instance_entry.path())? {
                let key_entry = key_entry?;
                if !key_entry.file_type()?.is_file() {
                    continue;
                }
                let filename = key_entry.file_name().to_string_lossy().to_string();
                let Some(device_id) = filename.strip_suffix(".pkcs8.pem") else {
                    continue;
                };
                entries.push(E2eePrivateKeyEntry {
                    instance: instance.clone(),
                    device_id: device_id.to_string(),
                    path: key_entry.path(),
                });
            }
        }
        entries.sort_by(|left, right| {
            left.instance
                .cmp(&right.instance)
                .then(left.device_id.cmp(&right.device_id))
        });
        Ok(entries)
    }

    fn bluesky_path(&self) -> PathBuf {
        self.root.join("bluesky.json")
    }
}

fn safe_path_component(value: &str) -> String {
    let component: String = value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':') {
                byte as char
            } else {
                '_'
            }
        })
        .collect();
    if component.is_empty() {
        "default".to_string()
    } else {
        component
    }
}

#[cfg(unix)]
fn set_owner_read_write(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_read_write(_path: &std::path::Path) -> Result<()> {
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

    #[test]
    fn stores_e2ee_private_keys_under_instance_and_device() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());

        let path = store
            .save_e2ee_private_key(
                "https://social.dais.social/",
                "laptop:2026",
                "private-key",
                false,
            )
            .unwrap();

        assert!(path.ends_with("e2ee/social.dais.social/laptop:2026.pkcs8.pem"));
        assert_eq!(
            store
                .load_e2ee_private_key("https://social.dais.social", "laptop:2026")
                .unwrap(),
            "private-key"
        );
        let entries = store.list_e2ee_private_keys().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].instance, "social.dais.social");
        assert_eq!(entries[0].device_id, "laptop:2026");
    }

    #[test]
    fn refuses_to_overwrite_e2ee_private_key_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        store
            .save_e2ee_private_key("https://social.skpt.cl", "phone", "one", false)
            .unwrap();

        assert!(store
            .save_e2ee_private_key("https://social.skpt.cl", "phone", "two", false)
            .is_err());
        store
            .save_e2ee_private_key("https://social.skpt.cl", "phone", "two", true)
            .unwrap();
        assert_eq!(
            store
                .load_e2ee_private_key("https://social.skpt.cl", "phone")
                .unwrap(),
            "two"
        );
    }
}
