use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use openmls::prelude::{
    tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize},
    *,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{MemoryStorage, RustCrypto};
use openmls_traits::OpenMlsProvider;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::RwLock};

const DAIS_MLS_ENVELOPE_VERSION: u8 = 2;
const DAIS_MLS_PROTOCOL: &str = "mls-rfc9420";

pub type MlsResult<T> = Result<T, MlsError>;

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("device {0} has not joined an MLS group")]
    MissingGroup(String),
    #[error("expected MLS welcome message")]
    MissingWelcome,
    #[error("expected MLS application message")]
    UnexpectedMessage,
    #[error("MLS operation failed: {0}")]
    OpenMls(String),
    #[error("MLS wire decoding failed: {0}")]
    Wire(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MlsDeviceMaterial {
    pub account_id: String,
    pub device_id: String,
    pub ciphersuite: String,
    pub signature_scheme: String,
    pub credential_identity: String,
    pub key_package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaisMlsEnvelope {
    pub v: u8,
    pub protocol: String,
    #[serde(rename = "groupId")]
    pub group_id: String,
    pub epoch: u64,
    #[serde(rename = "senderActorId")]
    pub sender_account_id: String,
    #[serde(rename = "senderDeviceId")]
    pub sender_device_id: String,
    pub ciphertext: String,
    #[serde(rename = "welcome", skip_serializing_if = "Option::is_none")]
    pub welcome: Option<DaisMlsWelcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaisMlsWelcome {
    pub message: String,
    #[serde(rename = "ratchetTree")]
    pub ratchet_tree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MlsDevicePrivateState {
    pub version: u8,
    pub account_id: String,
    pub device_id: String,
    pub signature_public_key: String,
    pub serialized_provider_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MlsDeviceState {
    pub version: u8,
    pub account_id: String,
    pub device_id: String,
    pub group_id: String,
    pub epoch: u64,
    pub signature_public_key: String,
    pub serialized_provider_state: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SerializedProviderState {
    version: u8,
    values: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct MlsPublicDevice {
    material: MlsDeviceMaterial,
    key_package: KeyPackage,
}

#[derive(Debug)]
pub struct MlsWelcome {
    welcome: Welcome,
    ratchet_tree: RatchetTreeIn,
}

#[derive(Debug, Clone)]
pub struct MlsCommit {
    message: MlsMessageOut,
}

#[derive(Debug)]
pub struct MlsDevice {
    account_id: String,
    device_id: String,
    provider: DaisOpenMlsProvider,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    key_package: KeyPackageBundle,
    group: Option<MlsGroup>,
}

#[derive(Debug)]
struct DaisOpenMlsProvider {
    crypto: RustCrypto,
    key_store: MemoryStorage,
}

impl Default for DaisOpenMlsProvider {
    fn default() -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: MemoryStorage::default(),
        }
    }
}

impl OpenMlsProvider for DaisOpenMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.key_store
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}

impl DaisOpenMlsProvider {
    fn export_state(&self) -> MlsResult<String> {
        let values = self
            .key_store
            .values
            .read()
            .map_err(|_| MlsError::OpenMls("MLS storage lock poisoned".to_string()))?;
        let values = values
            .iter()
            .map(|(key, value)| (BASE64.encode(key), BASE64.encode(value)))
            .collect();
        serde_json::to_string(&SerializedProviderState { version: 1, values })
            .map_err(|err| MlsError::Wire(err.to_string()))
    }

    fn import_state(serialized: &str) -> MlsResult<Self> {
        let state: SerializedProviderState =
            serde_json::from_str(serialized).map_err(|err| MlsError::Wire(err.to_string()))?;
        if state.version != 1 {
            return Err(MlsError::Wire(format!(
                "unsupported MLS provider state version {}",
                state.version
            )));
        }
        let mut values = std::collections::HashMap::new();
        for (key, value) in state.values {
            values.insert(
                BASE64
                    .decode(key)
                    .map_err(|err| MlsError::Wire(err.to_string()))?,
                BASE64
                    .decode(value)
                    .map_err(|err| MlsError::Wire(err.to_string()))?,
            );
        }
        Ok(Self {
            crypto: RustCrypto::default(),
            key_store: MemoryStorage {
                values: RwLock::new(values),
            },
        })
    }
}

impl MlsDevice {
    pub fn new(account_id: impl Into<String>, device_id: impl Into<String>) -> MlsResult<Self> {
        let account_id = account_id.into();
        let device_id = device_id.into();
        let provider = DaisOpenMlsProvider::default();
        let credential_identity = credential_identity(&account_id, &device_id);
        let signer = SignatureKeyPair::new(default_ciphersuite().signature_algorithm())
            .map_err(openmls_error)?;
        signer
            .store(provider.storage())
            .map_err(|err| MlsError::OpenMls(err.to_string()))?;
        let credential = CredentialWithKey {
            credential: BasicCredential::new(credential_identity).into(),
            signature_key: signer.public().into(),
        };
        let key_package = KeyPackage::builder()
            .build(
                default_ciphersuite(),
                &provider,
                &signer,
                credential.clone(),
            )
            .map_err(openmls_error)?;

        Ok(Self {
            account_id,
            device_id,
            provider,
            signer,
            credential,
            key_package,
            group: None,
        })
    }

    pub fn from_state(state: &MlsDeviceState) -> MlsResult<Self> {
        if state.version != 1 {
            return Err(MlsError::Wire(format!(
                "unsupported MLS device state version {}",
                state.version
            )));
        }
        let private_state = MlsDevicePrivateState {
            version: state.version,
            account_id: state.account_id.clone(),
            device_id: state.device_id.clone(),
            signature_public_key: state.signature_public_key.clone(),
            serialized_provider_state: state.serialized_provider_state.clone(),
        };
        let mut device = Self::from_private_state(&private_state)?;
        let decoded_group_id = BASE64
            .decode(&state.group_id)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let group_id = GroupId::from_slice(&decoded_group_id);
        let group = MlsGroup::load(device.provider.storage(), &group_id)
            .map_err(|err| MlsError::OpenMls(err.to_string()))?
            .ok_or_else(|| MlsError::MissingGroup(state.device_id.clone()))?;
        if group.epoch().as_u64() != state.epoch {
            return Err(MlsError::Wire(format!(
                "MLS state epoch mismatch: expected {}, loaded {}",
                state.epoch,
                group.epoch().as_u64()
            )));
        }
        device.group = Some(group);
        Ok(device)
    }

    pub fn from_private_state(state: &MlsDevicePrivateState) -> MlsResult<Self> {
        if state.version != 1 {
            return Err(MlsError::Wire(format!(
                "unsupported MLS device private state version {}",
                state.version
            )));
        }
        let provider = DaisOpenMlsProvider::import_state(&state.serialized_provider_state)?;
        let signature_public_key = BASE64
            .decode(&state.signature_public_key)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let signer = SignatureKeyPair::read(
            provider.storage(),
            &signature_public_key,
            default_ciphersuite().signature_algorithm(),
        )
        .ok_or_else(|| MlsError::Wire("MLS signature key missing from state".to_string()))?;
        let credential = CredentialWithKey {
            credential: BasicCredential::new(credential_identity(
                &state.account_id,
                &state.device_id,
            ))
            .into(),
            signature_key: signer.public().into(),
        };
        let key_package = KeyPackage::builder()
            .build(
                default_ciphersuite(),
                &provider,
                &signer,
                credential.clone(),
            )
            .map_err(openmls_error)?;

        Ok(Self {
            account_id: state.account_id.clone(),
            device_id: state.device_id.clone(),
            provider,
            signer,
            credential,
            key_package,
            group: None,
        })
    }

    pub fn public_device(&self) -> MlsResult<MlsPublicDevice> {
        Ok(MlsPublicDevice {
            material: MlsDeviceMaterial {
                account_id: self.account_id.clone(),
                device_id: self.device_id.clone(),
                ciphersuite: format!("{:?}", default_ciphersuite()),
                signature_scheme: format!("{:?}", default_ciphersuite().signature_algorithm()),
                credential_identity: BASE64
                    .encode(credential_identity(&self.account_id, &self.device_id)),
                key_package: BASE64.encode(
                    self.key_package
                        .key_package()
                        .tls_serialize_detached()
                        .map_err(openmls_error)?,
                ),
            },
            key_package: self.key_package.key_package().clone(),
        })
    }

    pub fn create_group(
        &mut self,
        group_id: impl AsRef<[u8]>,
        invitee: &MlsPublicDevice,
    ) -> MlsResult<MlsWelcome> {
        self.create_group_with_members(group_id, core::slice::from_ref(invitee))
    }

    pub fn create_group_with_members(
        &mut self,
        group_id: impl AsRef<[u8]>,
        invitees: &[MlsPublicDevice],
    ) -> MlsResult<MlsWelcome> {
        if invitees.is_empty() {
            return Err(MlsError::OpenMls(
                "MLS group requires at least one invitee".to_string(),
            ));
        }
        let mut group = MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &MlsGroupCreateConfig::default(),
            GroupId::from_slice(group_id.as_ref()),
            self.credential.clone(),
        )
        .map_err(openmls_error)?;
        let key_packages: Vec<KeyPackage> = invitees
            .iter()
            .map(|invitee| invitee.key_package.clone())
            .collect();
        let (_commit, welcome, _group_info) = group
            .add_members(&self.provider, &self.signer, key_packages.as_slice())
            .map_err(openmls_error)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;

        let ratchet_tree = group.export_ratchet_tree().into();
        self.group = Some(group);

        Ok(MlsWelcome {
            welcome: welcome_message_into_welcome(welcome)?,
            ratchet_tree,
        })
    }

    pub fn add_member(&mut self, invitee: &MlsPublicDevice) -> MlsResult<(MlsCommit, MlsWelcome)> {
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        let (commit, welcome, _group_info) = group
            .add_members(
                &self.provider,
                &self.signer,
                core::slice::from_ref(&invitee.key_package),
            )
            .map_err(openmls_error)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;

        Ok((
            MlsCommit { message: commit },
            MlsWelcome {
                welcome: welcome_message_into_welcome(welcome)?,
                ratchet_tree: group.export_ratchet_tree().into(),
            },
        ))
    }

    pub fn remove_member_at(&mut self, leaf_index: u32) -> MlsResult<MlsCommit> {
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        let (commit, _welcome, _group_info) = group
            .remove_members(
                &self.provider,
                &self.signer,
                &[LeafNodeIndex::new(leaf_index)],
            )
            .map_err(openmls_error)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;
        Ok(MlsCommit { message: commit })
    }

    pub fn apply_commit(&mut self, commit: MlsCommit) -> MlsResult<()> {
        let protocol_message = out_message_into_protocol_message(commit.message)?;
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        let processed = group
            .process_message(&self.provider, protocol_message)
            .map_err(openmls_error)?;

        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => group
                .merge_staged_commit(&self.provider, *staged_commit)
                .map_err(openmls_error),
            _ => Err(MlsError::UnexpectedMessage),
        }
    }

    pub fn join_group(&mut self, welcome: MlsWelcome) -> MlsResult<()> {
        let staged = StagedWelcome::new_from_welcome(
            &self.provider,
            &MlsGroupJoinConfig::default(),
            welcome.welcome,
            Some(welcome.ratchet_tree),
        )
        .map_err(openmls_error)?;
        self.group = Some(staged.into_group(&self.provider).map_err(openmls_error)?);
        Ok(())
    }

    pub fn encrypt_application_message(
        &mut self,
        plaintext: impl AsRef<[u8]>,
    ) -> MlsResult<DaisMlsEnvelope> {
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        let ciphertext = group
            .create_message(&self.provider, &self.signer, plaintext.as_ref())
            .map_err(openmls_error)?;
        Ok(DaisMlsEnvelope {
            v: DAIS_MLS_ENVELOPE_VERSION,
            protocol: DAIS_MLS_PROTOCOL.to_string(),
            group_id: BASE64.encode(group.group_id().as_slice()),
            epoch: group.epoch().as_u64(),
            sender_account_id: self.account_id.clone(),
            sender_device_id: self.device_id.clone(),
            ciphertext: BASE64.encode(ciphertext.tls_serialize_detached().map_err(openmls_error)?),
            welcome: None,
        })
    }

    pub fn decrypt_application_message(
        &mut self,
        envelope: &DaisMlsEnvelope,
    ) -> MlsResult<Vec<u8>> {
        if envelope.v != DAIS_MLS_ENVELOPE_VERSION || envelope.protocol != DAIS_MLS_PROTOCOL {
            return Err(MlsError::Wire(format!(
                "unsupported MLS envelope {} {}",
                envelope.v, envelope.protocol
            )));
        }

        let ciphertext = BASE64
            .decode(&envelope.ciphertext)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let message = MlsMessageIn::tls_deserialize(&mut ciphertext.as_slice())
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let protocol_message = message
            .try_into_protocol_message()
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        if self.group.is_none() {
            if let Some(welcome) = envelope.welcome.as_ref() {
                self.join_group(MlsWelcome::from_wire(welcome)?)?;
            }
        }
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        let processed = group
            .process_message(&self.provider, protocol_message)
            .map_err(openmls_error)?;

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => Ok(message.into_bytes()),
            _ => Err(MlsError::UnexpectedMessage),
        }
    }

    pub fn current_epoch(&self) -> MlsResult<u64> {
        self.group
            .as_ref()
            .map(|group| group.epoch().as_u64())
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))
    }

    pub fn export_state(&self) -> MlsResult<MlsDeviceState> {
        let group = self
            .group
            .as_ref()
            .ok_or_else(|| MlsError::MissingGroup(self.device_id.clone()))?;
        Ok(MlsDeviceState {
            version: 1,
            account_id: self.account_id.clone(),
            device_id: self.device_id.clone(),
            group_id: BASE64.encode(group.group_id().as_slice()),
            epoch: group.epoch().as_u64(),
            signature_public_key: BASE64.encode(self.signer.public()),
            serialized_provider_state: self.provider.export_state()?,
        })
    }

    pub fn export_private_state(&self) -> MlsResult<MlsDevicePrivateState> {
        Ok(MlsDevicePrivateState {
            version: 1,
            account_id: self.account_id.clone(),
            device_id: self.device_id.clone(),
            signature_public_key: BASE64.encode(self.signer.public()),
            serialized_provider_state: self.provider.export_state()?,
        })
    }
}

impl MlsPublicDevice {
    pub fn material(&self) -> &MlsDeviceMaterial {
        &self.material
    }

    pub fn from_material(material: MlsDeviceMaterial) -> MlsResult<Self> {
        let provider = DaisOpenMlsProvider::default();
        let decoded = BASE64
            .decode(&material.key_package)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let key_package = KeyPackageIn::tls_deserialize(&mut decoded.as_slice())
            .map_err(|err| MlsError::Wire(err.to_string()))?
            .validate(provider.crypto(), ProtocolVersion::Mls10)
            .map_err(openmls_error)?;
        Ok(Self {
            material,
            key_package,
        })
    }
}

impl MlsWelcome {
    pub fn to_wire(&self) -> MlsResult<DaisMlsWelcome> {
        Ok(DaisMlsWelcome {
            message: BASE64.encode(
                self.welcome
                    .tls_serialize_detached()
                    .map_err(openmls_error)?,
            ),
            ratchet_tree: BASE64.encode(
                self.ratchet_tree
                    .tls_serialize_detached()
                    .map_err(openmls_error)?,
            ),
        })
    }

    pub fn from_wire(wire: &DaisMlsWelcome) -> MlsResult<Self> {
        let welcome = BASE64
            .decode(&wire.message)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        let ratchet_tree = BASE64
            .decode(&wire.ratchet_tree)
            .map_err(|err| MlsError::Wire(err.to_string()))?;
        Ok(Self {
            welcome: Welcome::tls_deserialize(&mut welcome.as_slice())
                .map_err(|err| MlsError::Wire(err.to_string()))?,
            ratchet_tree: RatchetTreeIn::tls_deserialize(&mut ratchet_tree.as_slice())
                .map_err(|err| MlsError::Wire(err.to_string()))?,
        })
    }
}

fn default_ciphersuite() -> Ciphersuite {
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519
}

fn credential_identity(account_id: &str, device_id: &str) -> Vec<u8> {
    format!("{account_id}#{device_id}").into_bytes()
}

fn welcome_message_into_welcome(message: MlsMessageOut) -> MlsResult<Welcome> {
    let bytes = message.tls_serialize_detached().map_err(openmls_error)?;
    let message = MlsMessageIn::tls_deserialize(&mut bytes.as_slice())
        .map_err(|err| MlsError::Wire(err.to_string()))?;
    match message.extract() {
        MlsMessageBodyIn::Welcome(welcome) => Ok(welcome),
        _ => Err(MlsError::MissingWelcome),
    }
}

fn out_message_into_protocol_message(message: MlsMessageOut) -> MlsResult<ProtocolMessage> {
    let bytes = message.tls_serialize_detached().map_err(openmls_error)?;
    let message = MlsMessageIn::tls_deserialize(&mut bytes.as_slice())
        .map_err(|err| MlsError::Wire(err.to_string()))?;
    message
        .try_into_protocol_message()
        .map_err(|err| MlsError::Wire(err.to_string()))
}

fn openmls_error(error: impl core::fmt::Display) -> MlsError {
    MlsError::OpenMls(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mls_device_material_contains_serialized_key_package() {
        let alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");

        let public = alice.public_device().expect("public device");
        let material = public.material();

        assert_eq!(
            material.account_id,
            "https://social.dais.social/users/social"
        );
        assert_eq!(material.device_id, "alice-mac");
        assert!(!material.key_package.is_empty());
        let decoded = BASE64.decode(&material.key_package).expect("base64");
        assert!(KeyPackageIn::tls_deserialize(&mut decoded.as_slice()).is_ok());
    }

    #[test]
    fn mls_one_to_one_round_trips_application_message() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_public = bob.public_device().expect("bob public device");

        let welcome = alice
            .create_group("dais-mls-dm-alice-bob", &bob_public)
            .expect("create group");
        bob.join_group(welcome).expect("bob joins group");

        let envelope = alice
            .encrypt_application_message(b"private hello from dais")
            .expect("encrypt");
        let plaintext = bob
            .decrypt_application_message(&envelope)
            .expect("bob decrypts");

        assert_eq!(plaintext, b"private hello from dais");
        assert_eq!(alice.current_epoch().expect("alice epoch"), 1);
        assert_eq!(bob.current_epoch().expect("bob epoch"), 1);
        assert_eq!(envelope.v, DAIS_MLS_ENVELOPE_VERSION);
        assert_eq!(envelope.protocol, DAIS_MLS_PROTOCOL);

        let serialized = serde_json::to_value(&envelope).expect("serialize envelope");
        assert_eq!(serialized["protocol"], "mls-rfc9420");
        assert!(serialized.get("groupId").is_some());
        assert!(serialized.get("senderDeviceId").is_some());
        assert!(serialized.get("group_id").is_none());
        assert!(serialized.get("sender_device_id").is_none());
    }

    #[test]
    fn exported_mls_state_restores_decrypt_capability() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_public = bob.public_device().expect("bob public device");

        let welcome = alice
            .create_group("dais-mls-dm-persist-bob", &bob_public)
            .expect("create group");
        bob.join_group(welcome).expect("bob joins group");
        let bob_state = bob.export_state().expect("export bob state");
        drop(bob);

        let envelope = alice
            .encrypt_application_message(b"state survived restart")
            .expect("encrypt after bob export");
        let mut restored_bob = MlsDevice::from_state(&bob_state).expect("restore bob state");

        assert_eq!(
            restored_bob
                .decrypt_application_message(&envelope)
                .expect("restored bob decrypts"),
            b"state survived restart"
        );
    }

    #[test]
    fn exported_mls_state_restores_send_capability() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_public = bob.public_device().expect("bob public device");

        let welcome = alice
            .create_group("dais-mls-dm-persist-alice", &bob_public)
            .expect("create group");
        bob.join_group(welcome).expect("bob joins group");
        let alice_state = alice.export_state().expect("export alice state");
        drop(alice);

        let mut restored_alice = MlsDevice::from_state(&alice_state).expect("restore alice state");
        let envelope = restored_alice
            .encrypt_application_message(b"restored sender works")
            .expect("restored alice encrypts");

        assert_eq!(
            bob.decrypt_application_message(&envelope)
                .expect("bob decrypts restored sender"),
            b"restored sender works"
        );
    }

    #[test]
    fn exported_mls_state_rejects_stale_epoch_metadata() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_public = bob.public_device().expect("bob public device");

        let welcome = alice
            .create_group("dais-mls-dm-stale-epoch", &bob_public)
            .expect("create group");
        bob.join_group(welcome).expect("bob joins group");
        let mut bob_state = bob.export_state().expect("export bob state");
        bob_state.epoch += 1;

        assert!(MlsDevice::from_state(&bob_state).is_err());
    }

    #[test]
    fn first_contact_envelope_welcome_restores_private_device_into_group() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_private_state = bob.export_private_state().expect("bob private state");
        let bob_public = MlsPublicDevice::from_material(
            bob.public_device()
                .expect("bob public device")
                .material()
                .clone(),
        )
        .expect("bob public from material");
        drop(bob);

        let welcome = alice
            .create_group("dais-mls-dm-first-contact", &bob_public)
            .expect("create group");
        let mut envelope = alice
            .encrypt_application_message(b"welcome carries first contact")
            .expect("encrypt");
        envelope.welcome = Some(welcome.to_wire().expect("wire welcome"));

        let mut restored_bob =
            MlsDevice::from_private_state(&bob_private_state).expect("restore private state");
        assert_eq!(
            restored_bob
                .decrypt_application_message(&envelope)
                .expect("welcome joins and decrypts"),
            b"welcome carries first contact"
        );
        assert_eq!(
            restored_bob
                .export_state()
                .expect("export joined state")
                .epoch,
            envelope.epoch
        );

        let serialized = serde_json::to_value(&envelope).expect("serialize envelope");
        assert!(serialized["welcome"]["message"].as_str().is_some());
        assert!(serialized["welcome"]["ratchetTree"].as_str().is_some());
    }

    #[test]
    fn mls_rejects_wrong_protocol_and_malformed_ciphertext() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let bob_public = bob.public_device().expect("bob public device");

        let welcome = alice
            .create_group("dais-mls-dm-failure-check", &bob_public)
            .expect("create group");
        bob.join_group(welcome).expect("bob joins group");

        let envelope = alice
            .encrypt_application_message(b"private hello from dais")
            .expect("encrypt");

        let mut wrong_protocol = envelope.clone();
        wrong_protocol.protocol = "dais-mls-v1".to_string();
        assert!(bob.decrypt_application_message(&wrong_protocol).is_err());

        let mut malformed = envelope;
        malformed.ciphertext = "not base64".to_string();
        assert!(bob.decrypt_application_message(&malformed).is_err());
    }

    #[test]
    fn mls_removed_member_cannot_decrypt_future_message() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob device");
        let mut charlie = MlsDevice::new("https://third.example/users/social", "charlie-tablet")
            .expect("charlie device");

        let bob_public = bob.public_device().expect("bob public device");
        let bob_welcome = alice
            .create_group("dais-mls-group-remove-bob", &bob_public)
            .expect("create group");
        bob.join_group(bob_welcome).expect("bob joins");

        let charlie_public = charlie.public_device().expect("charlie public device");
        let (add_charlie_commit, charlie_welcome) =
            alice.add_member(&charlie_public).expect("add charlie");
        bob.apply_commit(add_charlie_commit)
            .expect("bob applies charlie add");
        charlie.join_group(charlie_welcome).expect("charlie joins");

        let remove_bob_commit = alice.remove_member_at(1).expect("remove bob");
        charlie
            .apply_commit(remove_bob_commit)
            .expect("charlie applies bob removal");

        let envelope = alice
            .encrypt_application_message(b"after bob was removed")
            .expect("encrypt after removal");

        assert_eq!(
            charlie
                .decrypt_application_message(&envelope)
                .expect("charlie decrypts"),
            b"after bob was removed"
        );
        assert!(bob.decrypt_application_message(&envelope).is_err());
    }

    #[test]
    fn mls_group_with_multiple_members_and_devices_survives_restart_and_removal() {
        let mut alice = MlsDevice::new("https://social.dais.social/users/social", "alice-mac")
            .expect("alice device");
        let mut bob_phone =
            MlsDevice::new("https://social.skpt.cl/users/social", "bob-phone").expect("bob phone");
        let mut bob_laptop = MlsDevice::new("https://social.skpt.cl/users/social", "bob-laptop")
            .expect("bob laptop");
        let mut charlie = MlsDevice::new("https://third.example/users/social", "charlie-tablet")
            .expect("charlie device");

        let invitees = vec![
            bob_phone.public_device().expect("bob phone public"),
            bob_laptop.public_device().expect("bob laptop public"),
            charlie.public_device().expect("charlie public"),
        ];
        let welcome = alice
            .create_group_with_members("dais-mls-group-multi-topology", &invitees)
            .expect("create multi-member group");
        let welcome_wire = welcome.to_wire().expect("welcome wire");
        for device in [&mut bob_phone, &mut bob_laptop, &mut charlie] {
            device
                .join_group(MlsWelcome::from_wire(&welcome_wire).expect("welcome from wire"))
                .expect("device joins multi-member group");
        }

        let first = alice
            .encrypt_application_message(b"multi-device group hello")
            .expect("encrypt first group message");
        for device in [&mut bob_phone, &mut bob_laptop, &mut charlie] {
            assert_eq!(
                device
                    .decrypt_application_message(&first)
                    .expect("member decrypts first message"),
                b"multi-device group hello"
            );
        }

        let charlie_state = charlie.export_state().expect("export charlie group state");
        let mut restored_charlie =
            MlsDevice::from_state(&charlie_state).expect("restore charlie group state");

        let remove_bob_laptop = alice.remove_member_at(2).expect("remove bob laptop");
        bob_phone
            .apply_commit(remove_bob_laptop.clone())
            .expect("bob phone applies removal");
        restored_charlie
            .apply_commit(remove_bob_laptop)
            .expect("charlie applies removal after restart");

        let second = alice
            .encrypt_application_message(b"after laptop removal")
            .expect("encrypt after removal");
        assert_eq!(
            bob_phone
                .decrypt_application_message(&second)
                .expect("remaining bob device decrypts"),
            b"after laptop removal"
        );
        assert_eq!(
            restored_charlie
                .decrypt_application_message(&second)
                .expect("restored charlie decrypts"),
            b"after laptop removal"
        );
        assert!(bob_laptop.decrypt_application_message(&second).is_err());
    }
}
