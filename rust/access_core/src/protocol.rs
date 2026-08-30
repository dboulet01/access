use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::{Path, PathBuf};

use coset::{
    CoseSign1, CoseSign1Builder, HeaderBuilder, RegisteredLabelWithPrivate, TaggedCborSerializable,
    iana,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIN_NONCE_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    AccessRequest,
    SessionOffer,
    SessionProof,
    AuthorizationGrant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizedStage {
    Hold,
    Approach,
    FinalApproach,
    SoftCapture,
    HardDock,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolClaims {
    pub message_type: MessageType,
    pub issuer: String,
    pub recipient: String,
    pub issued_at_s: i64,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    pub session_id: Option<String>,
    pub authorized_stage: Option<AuthorizedStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_nonce: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_s: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_profile_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_policy_bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_policy_bundle_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_policy_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CredentialClaims {
    pub issuer: String,
    pub subject: String,
    pub profile_id: String,
    pub credential_type: String,
    pub schema_id: String,
    pub issuer_group: String,
    pub issued_at_s: i64,
    pub expires_at_s: i64,
    pub status_checked_at_s: i64,
}

pub trait PayloadSigner: Send + Sync {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, String>;
}

struct LocalPayloadSigner(SigningKey);

impl PayloadSigner for LocalPayloadSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign(payload).to_bytes().to_vec())
    }
}

pub struct IdentityKey {
    key_id: String,
    verifying_key: VerifyingKey,
    signer: Box<dyn PayloadSigner>,
}

impl IdentityKey {
    pub fn from_seed(key_id: impl Into<String>, seed: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        Self {
            key_id: key_id.into(),
            verifying_key: signing_key.verifying_key(),
            signer: Box::new(LocalPayloadSigner(signing_key)),
        }
    }

    pub fn from_signer(
        key_id: impl Into<String>,
        verifying_key: VerifyingKey,
        signer: impl PayloadSigner + 'static,
    ) -> Self {
        Self {
            key_id: key_id.into(),
            verifying_key,
            signer: Box::new(signer),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.verifying_key
    }
}

#[derive(Default)]
pub struct TrustStore(HashMap<String, VerifyingKey>);

impl TrustStore {
    pub fn insert(&mut self, key_id: impl Into<String>, key: VerifyingKey) {
        self.0.insert(key_id.into(), key);
    }
}

/// Durable consumed-identifier storage used by replay protection.
///
/// Implementations must durably commit an identifier before returning success.
/// Returning an error causes the protected operation to fail closed.
pub trait ReplayStateBackend: Send {
    fn load(&mut self) -> Result<Vec<Vec<u8>>, String>;
    fn append_and_sync(&mut self, value: &[u8]) -> Result<(), String>;
}

struct FileReplayStateBackend {
    path: PathBuf,
}

impl FileReplayStateBackend {
    fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Ok(Self {
            path: path.to_owned(),
        })
    }
}

impl ReplayStateBackend for FileReplayStateBackend {
    fn load(&mut self) -> Result<Vec<Vec<u8>>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        BufReader::new(file)
            .lines()
            .map(|line| {
                let encoded = line.map_err(|error| error.to_string())?;
                hex::decode(encoded).map_err(|error| error.to_string())
            })
            .collect()
    }

    fn append_and_sync(&mut self, value: &[u8]) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| error.to_string())?;
        writeln!(file, "{}", hex::encode(value)).map_err(|error| error.to_string())?;
        file.sync_data().map_err(|error| error.to_string())
    }
}

/// In-memory replay protection with an optional durable-state backend.
///
/// A production integration supplies a rollback-resistant implementation of
/// `ReplayStateBackend`. `persistent` retains the synchronized append-only file
/// backend used by the executable test suites.
#[derive(Default)]
pub struct ReplayCache {
    consumed: HashSet<Vec<u8>>,
    backend: Option<Box<dyn ReplayStateBackend>>,
}

impl ReplayCache {
    pub fn persistent(path: impl AsRef<Path>) -> Result<Self, ProtocolError> {
        let backend = FileReplayStateBackend::new(path).map_err(ProtocolError::PersistentState)?;
        Self::with_backend(backend)
    }

    pub fn with_backend(
        mut backend: impl ReplayStateBackend + 'static,
    ) -> Result<Self, ProtocolError> {
        let consumed = backend
            .load()
            .map_err(ProtocolError::PersistentState)?
            .into_iter()
            .collect();
        Ok(Self {
            consumed,
            backend: Some(Box::new(backend)),
        })
    }

    pub fn consume(&mut self, nonce: &[u8]) -> Result<(), ProtocolError> {
        if self.consumed.contains(nonce) {
            return Err(ProtocolError::Replay);
        }
        if let Some(backend) = &mut self.backend {
            backend
                .append_and_sync(nonce)
                .map_err(ProtocolError::PersistentState)?;
        }
        self.consumed.insert(nonce.to_vec());
        Ok(())
    }

    pub(crate) fn reset_ephemeral(&mut self) {
        if self.backend.is_none() {
            self.consumed.clear();
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEnvelope {
    pub signer: String,
    pub claims: ProtocolClaims,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
    #[error("invalid COSE_Sign1 envelope")]
    InvalidEnvelope,
    #[error("only EdDSA is accepted")]
    InvalidAlgorithm,
    #[error("unknown or malformed key identifier")]
    UntrustedSigner,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("invalid CBOR claims")]
    InvalidClaims,
    #[error("protected key identifier and claim issuer differ")]
    IssuerMismatch,
    #[error("message recipient does not match local identity")]
    RecipientMismatch,
    #[error("message is outside the freshness window")]
    Stale,
    #[error("nonce must contain at least 128 bits")]
    WeakNonce,
    #[error("nonce has already been consumed")]
    Replay,
    #[error("credential subject does not match the expected holder")]
    SubjectMismatch,
    #[error("credential is not currently valid")]
    CredentialExpired,
    #[error("signing operation failed: {0}")]
    SigningFailed(String),
    #[error("persistent replay state failed: {0}")]
    PersistentState(String),
}

pub fn issue_credential(
    claims: &CredentialClaims,
    issuer: &IdentityKey,
) -> Result<Vec<u8>, ProtocolError> {
    if claims.issuer != issuer.key_id {
        return Err(ProtocolError::IssuerMismatch);
    }

    let mut payload = Vec::new();
    ciborium::into_writer(claims, &mut payload).map_err(|_| ProtocolError::InvalidClaims)?;
    sign_payload(payload, issuer, b"access-credential-v1")
}

pub fn verify_credential(
    encoded: &[u8],
    trust_store: &TrustStore,
    expected_subject: &str,
    now_s: i64,
) -> Result<CredentialClaims, ProtocolError> {
    let (signer, payload) = verify_signed_payload(encoded, trust_store, b"access-credential-v1")?;
    let claims: CredentialClaims =
        ciborium::from_reader(Cursor::new(payload)).map_err(|_| ProtocolError::InvalidClaims)?;
    if claims.issuer != signer {
        return Err(ProtocolError::IssuerMismatch);
    }
    if claims.subject != expected_subject {
        return Err(ProtocolError::SubjectMismatch);
    }
    if now_s < claims.issued_at_s || now_s > claims.expires_at_s {
        return Err(ProtocolError::CredentialExpired);
    }
    Ok(claims)
}

pub fn sign_envelope(
    claims: &ProtocolClaims,
    identity: &IdentityKey,
) -> Result<Vec<u8>, ProtocolError> {
    if claims.issuer != identity.key_id {
        return Err(ProtocolError::IssuerMismatch);
    }

    let mut payload = Vec::new();
    ciborium::into_writer(claims, &mut payload).map_err(|_| ProtocolError::InvalidClaims)?;
    sign_payload(payload, identity, b"docking-identity-v1")
}

fn sign_payload(
    payload: Vec<u8>,
    identity: &IdentityKey,
    external_aad: &[u8],
) -> Result<Vec<u8>, ProtocolError> {
    let protected = HeaderBuilder::new()
        .algorithm(iana::Algorithm::EdDSA)
        .key_id(identity.key_id.as_bytes().to_vec())
        .build();
    let message = CoseSign1Builder::new()
        .protected(protected)
        .payload(payload)
        .try_create_signature(external_aad, |to_be_signed| {
            identity.signer.sign(to_be_signed)
        })
        .map_err(ProtocolError::SigningFailed)?
        .build();

    message
        .to_tagged_vec()
        .map_err(|_| ProtocolError::InvalidEnvelope)
}

pub fn verify_envelope(
    encoded: &[u8],
    trust_store: &TrustStore,
    expected_recipient: &str,
    replay_cache: &mut ReplayCache,
    now_s: i64,
    max_clock_skew_s: i64,
) -> Result<VerifiedEnvelope, ProtocolError> {
    let (signer, payload) = verify_signed_payload(encoded, trust_store, b"docking-identity-v1")?;
    let claims: ProtocolClaims =
        ciborium::from_reader(Cursor::new(payload)).map_err(|_| ProtocolError::InvalidClaims)?;
    if claims.issuer != signer {
        return Err(ProtocolError::IssuerMismatch);
    }
    if claims.recipient != expected_recipient {
        return Err(ProtocolError::RecipientMismatch);
    }
    if now_s.abs_diff(claims.issued_at_s) > max_clock_skew_s.unsigned_abs() {
        return Err(ProtocolError::Stale);
    }
    if claims.nonce.len() < MIN_NONCE_BYTES {
        return Err(ProtocolError::WeakNonce);
    }
    replay_cache.consume(&claims.nonce)?;

    Ok(VerifiedEnvelope { signer, claims })
}

fn verify_signed_payload(
    encoded: &[u8],
    trust_store: &TrustStore,
    external_aad: &[u8],
) -> Result<(String, Vec<u8>), ProtocolError> {
    let message =
        CoseSign1::from_tagged_slice(encoded).map_err(|_| ProtocolError::InvalidEnvelope)?;
    let header = &message.protected.header;
    if header.alg != Some(RegisteredLabelWithPrivate::Assigned(iana::Algorithm::EdDSA)) {
        return Err(ProtocolError::InvalidAlgorithm);
    }
    let signer =
        String::from_utf8(header.key_id.clone()).map_err(|_| ProtocolError::UntrustedSigner)?;
    let verifying_key = trust_store
        .0
        .get(&signer)
        .ok_or(ProtocolError::UntrustedSigner)?;

    message
        .verify_signature(external_aad, |signature, to_be_signed| {
            let signature = Signature::from_slice(signature)?;
            verifying_key.verify_strict(to_be_signed, &signature)
        })
        .map_err(|_| ProtocolError::InvalidSignature)?;

    Ok((signer, message.payload.ok_or(ProtocolError::InvalidClaims)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct WriteFailingBackend;

    impl ReplayStateBackend for WriteFailingBackend {
        fn load(&mut self) -> Result<Vec<Vec<u8>>, String> {
            Ok(Vec::new())
        }

        fn append_and_sync(&mut self, _value: &[u8]) -> Result<(), String> {
            Err("durable commit failed".into())
        }
    }

    #[test]
    fn persistent_replay_cache_survives_restart() {
        let path = std::env::temp_dir().join(format!(
            "docking-replay-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let mut first = ReplayCache::persistent(&path).unwrap();
        first.consume(b"persisted-grant-id").unwrap();
        first.reset_ephemeral();
        assert_eq!(
            first.consume(b"persisted-grant-id"),
            Err(ProtocolError::Replay)
        );

        let mut restarted = ReplayCache::persistent(&path).unwrap();
        assert_eq!(
            restarted.consume(b"persisted-grant-id"),
            Err(ProtocolError::Replay)
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn persistent_replay_cache_rejects_corrupt_journal() {
        let path = std::env::temp_dir().join(format!(
            "docking-replay-corrupt-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "not-hex\n").unwrap();

        assert!(matches!(
            ReplayCache::persistent(&path),
            Err(ProtocolError::PersistentState(_))
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_cache_fails_closed_when_durable_commit_fails() {
        let mut cache = ReplayCache::with_backend(WriteFailingBackend).unwrap();
        assert!(matches!(
            cache.consume(b"grant-id"),
            Err(ProtocolError::PersistentState(_))
        ));
        assert!(!cache.consumed.contains(b"grant-id".as_slice()));
    }

    fn claims() -> ProtocolClaims {
        ProtocolClaims {
            message_type: MessageType::AccessRequest,
            issuer: "chaser-1".into(),
            recipient: "target-1".into(),
            issued_at_s: 1_000,
            nonce: b"0123456789abcdef".to_vec(),
            session_id: None,
            authorized_stage: None,
            challenge_nonce: None,
            credentials: vec![],
            grant_id: None,
            expires_at_s: None,
            protocol_profile_id: None,
            protocol_profile_version: None,
            rule_id: None,
            authorization_policy_bundle_id: None,
            authorization_policy_bundle_version: None,
            authorization_policy_sha256: None,
        }
    }

    fn fixture() -> (IdentityKey, TrustStore) {
        let identity = IdentityKey::from_seed("chaser-1", [7; 32]);
        let mut trust = TrustStore::default();
        trust.insert(identity.key_id(), identity.verifying_key());
        (identity, trust)
    }

    #[test]
    fn verifies_valid_envelope_only_once() {
        let (identity, trust) = fixture();
        let encoded = sign_envelope(&claims(), &identity).unwrap();
        let mut replay = ReplayCache::default();

        let verified =
            verify_envelope(&encoded, &trust, "target-1", &mut replay, 1_000, 30).unwrap();
        assert_eq!(verified.signer, "chaser-1");
        assert_eq!(
            verify_envelope(&encoded, &trust, "target-1", &mut replay, 1_000, 30),
            Err(ProtocolError::Replay)
        );
    }

    #[test]
    fn rejects_tampering() {
        let (identity, trust) = fixture();
        let mut encoded = sign_envelope(&claims(), &identity).unwrap();
        *encoded.last_mut().unwrap() ^= 1;

        assert_eq!(
            verify_envelope(
                &encoded,
                &trust,
                "target-1",
                &mut ReplayCache::default(),
                1_000,
                30,
            ),
            Err(ProtocolError::InvalidSignature)
        );
    }

    #[test]
    fn verifies_issuer_signed_credential_and_holder_binding() {
        let issuer = IdentityKey::from_seed("orbital-registry", [9; 32]);
        let mut trust = TrustStore::default();
        trust.insert(issuer.key_id(), issuer.verifying_key());
        let claims = CredentialClaims {
            issuer: issuer.key_id().into(),
            subject: "chaser-1".into(),
            profile_id: "registered-vehicle-v1".into(),
            credential_type: "vehicle_registration".into(),
            schema_id: "space:vehicle-registration:v1".into(),
            issuer_group: "recognized-registrars".into(),
            issued_at_s: 900,
            expires_at_s: 1_100,
            status_checked_at_s: 990,
        };
        let encoded = issue_credential(&claims, &issuer).unwrap();

        assert_eq!(
            verify_credential(&encoded, &trust, "chaser-1", 1_000),
            Ok(claims.clone())
        );
        assert_eq!(
            verify_credential(&encoded, &trust, "other-holder", 1_000),
            Err(ProtocolError::SubjectMismatch)
        );
        assert_eq!(
            verify_credential(&encoded, &trust, "chaser-1", 1_101),
            Err(ProtocolError::CredentialExpired)
        );
    }
}
