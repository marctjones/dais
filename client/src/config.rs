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

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct MlsGroupStateFile {
    pub version: u8,
    pub instance_url: String,
    pub local_actor_id: String,
    pub device_id: String,
    pub group_id: String,
    pub epoch: u64,
    pub serialized_group_state: String,
    pub recovery_status: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct MlsDeviceStateFile {
    pub version: u8,
    pub instance_url: String,
    pub local_actor_id: String,
    pub device_id: String,
    pub serialized_device_state: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DecryptedMessageFile {
    pub instance_url: String,
    pub message_id: String,
    pub plaintext: String,
    pub protocol: String,
    pub cached_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MlsGroupStateEntry {
    pub instance: String,
    pub device_id: String,
    pub group_id: String,
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
            .join(self.e2ee_instance_dir_name(instance_url))
            .join(format!("{}.pkcs8.pem", safe_path_component(device_id)))
    }

    pub fn e2ee_instance_dir_name(&self, instance_url: &str) -> String {
        safe_path_component(instance_url)
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

    pub fn mls_group_state_path(
        &self,
        instance_url: &str,
        device_id: &str,
        group_id: &str,
    ) -> PathBuf {
        self.root
            .join("mls")
            .join(safe_path_component(instance_url))
            .join(safe_path_component(device_id))
            .join(format!("{}.json", safe_path_component(group_id)))
    }

    pub fn mls_device_state_path(&self, instance_url: &str, device_id: &str) -> PathBuf {
        self.root
            .join("mls-devices")
            .join(safe_path_component(instance_url))
            .join(format!("{}.json", safe_path_component(device_id)))
    }

    pub fn decrypted_message_path(&self, instance_url: &str, message_id: &str) -> PathBuf {
        self.root
            .join("decrypted-messages")
            .join(safe_path_component(instance_url))
            .join(format!("{}.json", safe_path_component(message_id)))
    }

    pub fn save_decrypted_message(
        &self,
        instance_url: &str,
        message_id: &str,
        plaintext: &str,
        protocol: &str,
    ) -> Result<PathBuf> {
        let normalized_instance_url = normalize_instance_url(instance_url);
        let path = self.decrypted_message_path(&normalized_instance_url, message_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let state = DecryptedMessageFile {
            instance_url: normalized_instance_url,
            message_id: message_id.to_string(),
            plaintext: plaintext.to_string(),
            protocol: protocol.to_string(),
            cached_at: unix_timestamp_label(),
        };
        let content = serde_json::to_string_pretty(&state)?;
        fs::write(&path, content)?;
        set_owner_read_write(&path)?;
        Ok(path)
    }

    #[allow(dead_code)]
    pub fn load_decrypted_message(
        &self,
        instance_url: &str,
        message_id: &str,
    ) -> Result<DecryptedMessageFile> {
        let path = self.decrypted_message_path(instance_url, message_id);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("decrypted message cache not found at {}", path.display()))?;
        let state: DecryptedMessageFile = serde_json::from_str(&content)
            .with_context(|| format!("invalid decrypted message cache at {}", path.display()))?;
        if normalize_instance_url(&state.instance_url) != normalize_instance_url(instance_url)
            || state.message_id != message_id
        {
            anyhow::bail!("decrypted message cache identity does not match requested message");
        }
        Ok(state)
    }

    pub fn save_mls_device_state(
        &self,
        state: &MlsDeviceStateFile,
        force: bool,
    ) -> Result<PathBuf> {
        if state.version != 1 {
            anyhow::bail!("unsupported MLS device state version {}", state.version);
        }
        if state.device_id.trim().is_empty() {
            anyhow::bail!("MLS device state device_id is required");
        }
        let path = self.mls_device_state_path(&state.instance_url, &state.device_id);
        if path.exists() && !force {
            anyhow::bail!(
                "{} already exists; pass --force to overwrite",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(state)?;
        fs::write(&path, content)?;
        set_owner_read_write(&path)?;
        Ok(path)
    }

    pub fn load_mls_device_state(
        &self,
        instance_url: &str,
        device_id: &str,
    ) -> Result<MlsDeviceStateFile> {
        let path = self.mls_device_state_path(instance_url, device_id);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("MLS device state not found at {}", path.display()))?;
        let state: MlsDeviceStateFile = serde_json::from_str(&content)
            .with_context(|| format!("invalid MLS device state at {}", path.display()))?;
        if state.version != 1 {
            anyhow::bail!("unsupported MLS device state version {}", state.version);
        }
        if safe_path_component(&state.instance_url) != safe_path_component(instance_url)
            || state.device_id != device_id
        {
            anyhow::bail!("MLS device state identity does not match requested instance/device");
        }
        Ok(state)
    }

    pub fn save_mls_group_state(&self, state: &MlsGroupStateFile, force: bool) -> Result<PathBuf> {
        if state.version != 1 {
            anyhow::bail!("unsupported MLS state version {}", state.version);
        }
        if state.device_id.trim().is_empty() {
            anyhow::bail!("MLS state device_id is required");
        }
        if state.group_id.trim().is_empty() {
            anyhow::bail!("MLS state group_id is required");
        }
        let path =
            self.mls_group_state_path(&state.instance_url, &state.device_id, &state.group_id);
        if path.exists() && !force {
            anyhow::bail!(
                "{} already exists; pass --force to overwrite",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(state)?;
        fs::write(&path, content)?;
        set_owner_read_write(&path)?;
        Ok(path)
    }

    pub fn load_mls_group_state(
        &self,
        instance_url: &str,
        device_id: &str,
        group_id: &str,
    ) -> Result<MlsGroupStateFile> {
        let path = self.mls_group_state_path(instance_url, device_id, group_id);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("MLS group state not found at {}", path.display()))?;
        let state: MlsGroupStateFile = serde_json::from_str(&content)
            .with_context(|| format!("invalid MLS group state at {}", path.display()))?;
        if state.version != 1 {
            anyhow::bail!("unsupported MLS state version {}", state.version);
        }
        if safe_path_component(&state.instance_url) != safe_path_component(instance_url)
            || state.device_id != device_id
            || state.group_id != group_id
        {
            anyhow::bail!(
                "MLS group state identity does not match requested instance/device/group"
            );
        }
        Ok(state)
    }

    pub fn list_mls_group_states(&self) -> Result<Vec<MlsGroupStateEntry>> {
        let root = self.root.join("mls");
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
            for device_entry in fs::read_dir(instance_entry.path())? {
                let device_entry = device_entry?;
                if !device_entry.file_type()?.is_dir() {
                    continue;
                }
                let device_id = device_entry.file_name().to_string_lossy().to_string();
                for state_entry in fs::read_dir(device_entry.path())? {
                    let state_entry = state_entry?;
                    if !state_entry.file_type()?.is_file() {
                        continue;
                    }
                    let filename = state_entry.file_name().to_string_lossy().to_string();
                    let Some(group_id) = filename.strip_suffix(".json") else {
                        continue;
                    };
                    entries.push(MlsGroupStateEntry {
                        instance: instance.clone(),
                        device_id: device_id.clone(),
                        group_id: group_id.to_string(),
                        path: state_entry.path(),
                    });
                }
            }
        }
        entries.sort_by(|left, right| {
            left.instance
                .cmp(&right.instance)
                .then(left.device_id.cmp(&right.device_id))
                .then(left.group_id.cmp(&right.group_id))
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

fn normalize_instance_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn unix_timestamp_label() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
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
    use super::{BlueskyConfig, ConfigStore, MlsDeviceStateFile, MlsGroupStateFile};

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

    #[test]
    fn stores_mls_group_state_under_instance_device_and_group() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        let state = MlsGroupStateFile {
            version: 1,
            instance_url: "https://social.dais.social".to_string(),
            local_actor_id: "https://social.dais.social/users/social".to_string(),
            device_id: "mac:2026".to_string(),
            group_id: "mls-group-1".to_string(),
            epoch: 3,
            serialized_group_state: "serialized-openmls-state".to_string(),
            recovery_status: "available".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
        };

        let path = store.save_mls_group_state(&state, false).unwrap();

        assert!(path.ends_with("mls/social.dais.social/mac:2026/mls-group-1.json"));
        assert_eq!(
            store
                .load_mls_group_state("https://social.dais.social", "mac:2026", "mls-group-1")
                .unwrap(),
            state
        );
        let entries = store.list_mls_group_states().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].instance, "social.dais.social");
        assert_eq!(entries[0].device_id, "mac:2026");
        assert_eq!(entries[0].group_id, "mls-group-1");
    }

    #[test]
    fn stores_mls_device_state_under_instance_and_device() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        let state = MlsDeviceStateFile {
            version: 1,
            instance_url: "https://social.dais.social".to_string(),
            local_actor_id: "https://social.dais.social/users/social".to_string(),
            device_id: "mac:2026".to_string(),
            serialized_device_state: "serialized-openmls-device-state".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
        };

        let path = store.save_mls_device_state(&state, false).unwrap();

        assert!(path.ends_with("mls-devices/social.dais.social/mac:2026.json"));
        assert_eq!(
            store
                .load_mls_device_state("https://social.dais.social", "mac:2026")
                .unwrap(),
            state
        );
        assert!(store.save_mls_device_state(&state, false).is_err());
        assert!(store
            .load_mls_device_state("https://social.dais.social", "other-device")
            .is_err());
    }

    #[test]
    fn stores_decrypted_messages_under_instance_and_message_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        let message_id = "https://social.dais.social/users/social/e2ee/messages/1";

        let path = store
            .save_decrypted_message(
                "https://social.skpt.cl/",
                message_id,
                "hello from MLS",
                "mls-rfc9420",
            )
            .unwrap();

        assert!(path.ends_with(
            "decrypted-messages/social.skpt.cl/social.dais.social_users_social_e2ee_messages_1.json"
        ));
        let cached = store
            .load_decrypted_message("https://social.skpt.cl", message_id)
            .unwrap();
        assert_eq!(cached.instance_url, "https://social.skpt.cl");
        assert_eq!(cached.message_id, message_id);
        assert_eq!(cached.plaintext, "hello from MLS");
        assert_eq!(cached.protocol, "mls-rfc9420");
        assert!(!cached.cached_at.is_empty());
    }

    #[test]
    fn mls_group_state_rejects_mismatched_identity_and_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().to_path_buf());
        let state = MlsGroupStateFile {
            version: 1,
            instance_url: "https://social.skpt.cl".to_string(),
            local_actor_id: "https://social.skpt.cl/users/social".to_string(),
            device_id: "phone".to_string(),
            group_id: "group".to_string(),
            epoch: 1,
            serialized_group_state: "one".to_string(),
            recovery_status: "available".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
        };

        store.save_mls_group_state(&state, false).unwrap();
        assert!(store.save_mls_group_state(&state, false).is_err());
        assert!(store
            .load_mls_group_state("https://social.skpt.cl", "other-phone", "group")
            .is_err());

        let mut updated = state;
        updated.epoch = 2;
        updated.serialized_group_state = "two".to_string();
        store.save_mls_group_state(&updated, true).unwrap();
        assert_eq!(
            store
                .load_mls_group_state("https://social.skpt.cl", "phone", "group")
                .unwrap()
                .epoch,
            2
        );
    }
}
