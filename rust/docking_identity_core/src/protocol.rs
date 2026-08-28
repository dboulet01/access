use std::collections::{HashMap, HashSet};
use std::io::Cursor;

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
    IdentityRequest,
    SessionOffer,
    SessionProof,
    AuthorizationGrant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizedStage {
    Hold,
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
}

pub struct IdentityKey {
    key_id: String,
    signing_key: SigningKey,
}

impl IdentityKey {
    pub fn from_seed(key_id: impl Into<String>, seed: [u8; 32]) -> Self {
        Self {
            key_id: key_id.into(),
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

#[derive(Default)]
pub struct TrustStore(HashMap<String, VerifyingKey>);

impl TrustStore {
    pub fn insert(&mut self, key_id: impl Into<String>, key: VerifyingKey) {
        self.0.insert(key_id.into(), key);
    }
}

#[derive(Default)]
pub struct ReplayCache(HashSet<Vec<u8>>);

impl ReplayCache {
    fn consume(&mut self, nonce: &[u8]) -> Result<(), ProtocolError> {
        if !self.0.insert(nonce.to_vec()) {
            return Err(ProtocolError::Replay);
        }
        Ok(())
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
    let protected = HeaderBuilder::new()
        .algorithm(iana::Algorithm::EdDSA)
        .key_id(identity.key_id.as_bytes().to_vec())
        .build();
    let message = CoseSign1Builder::new()
        .protected(protected)
        .payload(payload)
        .create_signature(b"docking-identity-v1", |to_be_signed| {
            identity.signing_key.sign(to_be_signed).to_bytes().to_vec()
        })
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
        .verify_signature(b"docking-identity-v1", |signature, to_be_signed| {
            let signature = Signature::from_slice(signature)?;
            verifying_key.verify_strict(to_be_signed, &signature)
        })
        .map_err(|_| ProtocolError::InvalidSignature)?;

    let payload = message.payload.ok_or(ProtocolError::InvalidClaims)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> ProtocolClaims {
        ProtocolClaims {
            message_type: MessageType::IdentityRequest,
            issuer: "chaser-1".into(),
            recipient: "target-1".into(),
            issued_at_s: 1_000,
            nonce: b"0123456789abcdef".to_vec(),
            session_id: None,
            authorized_stage: None,
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
}
