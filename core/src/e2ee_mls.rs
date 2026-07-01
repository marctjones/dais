use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use openmls::prelude::{
    tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize},
    *,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use serde::{Deserialize, Serialize};

const DAIS_MLS_ENVELOPE_VERSION: u8 = 2;
const DAIS_MLS_PROTOCOL: &str = "MLS-1.0-OpenMLS";

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
    pub group_id: String,
    pub epoch: u64,
    pub sender_account_id: String,
    pub sender_device_id: String,
    pub ciphertext: String,
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

#[derive(Debug)]
pub struct MlsCommit {
    message: MlsMessageOut,
}

#[derive(Debug)]
pub struct MlsDevice {
    account_id: String,
    device_id: String,
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    key_package: KeyPackageBundle,
    group: Option<MlsGroup>,
}

impl MlsDevice {
    pub fn new(account_id: impl Into<String>, device_id: impl Into<String>) -> MlsResult<Self> {
        let account_id = account_id.into();
        let device_id = device_id.into();
        let provider = OpenMlsRustCrypto::default();
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
        let mut group = MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &MlsGroupCreateConfig::default(),
            GroupId::from_slice(group_id.as_ref()),
            self.credential.clone(),
        )
        .map_err(openmls_error)?;
        let (_commit, welcome, _group_info) = group
            .add_members(
                &self.provider,
                &self.signer,
                core::slice::from_ref(&invitee.key_package),
            )
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
}

impl MlsPublicDevice {
    pub fn material(&self) -> &MlsDeviceMaterial {
        &self.material
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
}
