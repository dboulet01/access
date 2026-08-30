use serde::Serialize;
use std::path::Path;
use thiserror::Error;

use crate::{
    AccessDecision, AccessPolicyError, AuthorizationContext, AuthorizedStage, CedarPolicyEngine,
    CredentialClaims, DockingEvent, DockingState, IdentityKey, MessageType, ProtocolClaims,
    ProtocolError, ProtocolProfile, ProtocolProfileError, ReadinessEvidence, ReplayCache,
    TransitionError, TrustStore, VerifiedCredentialEvidence, VerifiedEnvelope, issue_credential,
    reduce, sign_envelope, verify_credential, verify_envelope,
};

const MAX_CLOCK_SKEW_S: i64 = 30;

/// Supplies cryptographically secure random bytes to the ACCESS session engine.
///
/// Flight integrations can provide a platform-qualified random source. The
/// default implementation uses the operating system source through `getrandom`.
pub trait RandomSource: Send + Sync {
    fn fill(&self, destination: &mut [u8]) -> Result<(), String>;
}

#[derive(Default)]
pub struct OsRandomSource;

impl RandomSource for OsRandomSource {
    fn fill(&self, destination: &mut [u8]) -> Result<(), String> {
        getrandom::fill(destination).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccessScenario {
    #[default]
    Nominal,
    ExpiredCredential,
    CorridorViolation,
    LatchNotReady,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccessEvent {
    pub code: String,
    pub detail: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub message_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TransitionOutcome {
    pub approved: bool,
    pub previous_state: u8,
    pub resulting_state: u8,
    pub reason: String,
    pub session_id: Option<String>,
    pub protocol_profile_id: Option<String>,
    pub protocol_profile_version: Option<u64>,
    pub rule_id: Option<String>,
    pub grant_id: Option<String>,
    pub entitlement_ttl_s: Option<u64>,
    pub grant_expires_at_s: Option<i64>,
    pub signed_grant_hex: Option<String>,
    pub authorization_decision: Option<AccessDecision>,
    pub events: Vec<AccessEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionOutcome {
    pub session_id: String,
    pub protocol_profile_id: String,
    pub protocol_profile_version: u64,
    pub authorization_decision: AccessDecision,
    pub events: Vec<AccessEvent>,
}

pub struct AccessEngineConfig {
    pub protocol_profile: ProtocolProfile,
    pub authorization_policy_engine: CedarPolicyEngine,
    pub trust_bundle_id: String,
    pub trust_bundle_version: u64,
    pub trust_bundle_issued_at_s: i64,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("protocol verification failed: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("protected transition failed: {0}")]
    Transition(#[from] TransitionError),
    #[error("session challenge or identifier binding failed")]
    SessionBinding,
    #[error("credential presentation is missing")]
    CredentialMissing,
    #[error("authorization grant does not match the requested stage")]
    GrantMismatch,
    #[error("ACCESS authorization policy evaluation failed: {0}")]
    AuthorizationPolicy(#[from] AccessPolicyError),
    #[error("protocol policy validation failed: {0}")]
    ProtocolProfile(#[from] ProtocolProfileError),
    #[error("operating-system random source failed")]
    RandomSource,
}

pub struct AccessEngine {
    chaser: IdentityKey,
    station: IdentityKey,
    credential_issuer: IdentityKey,
    station_peer_trust: TrustStore,
    chaser_peer_trust: TrustStore,
    credential_trust: TrustStore,
    gate_trust: TrustStore,
    station_replay: ReplayCache,
    chaser_replay: ReplayCache,
    gate_replay: ReplayCache,
    consumed_grants: ReplayCache,
    config: AccessEngineConfig,
    state: DockingState,
    authorization: AuthorizationContext,
    session_id: Option<String>,
    verified_credentials: Vec<VerifiedCredentialEvidence>,
    holder_proof_at_s: Option<i64>,
    scenario: AccessScenario,
    random_source: Box<dyn RandomSource>,
}

impl AccessEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chaser: IdentityKey,
        station: IdentityKey,
        credential_issuer: IdentityKey,
        station_peer_trust: TrustStore,
        chaser_peer_trust: TrustStore,
        credential_trust: TrustStore,
        gate_trust: TrustStore,
        config: AccessEngineConfig,
    ) -> Self {
        Self {
            chaser,
            station,
            credential_issuer,
            station_peer_trust,
            chaser_peer_trust,
            credential_trust,
            gate_trust,
            station_replay: ReplayCache::default(),
            chaser_replay: ReplayCache::default(),
            gate_replay: ReplayCache::default(),
            consumed_grants: ReplayCache::default(),
            config,
            state: DockingState::Hold,
            authorization: AuthorizationContext::default(),
            session_id: None,
            verified_credentials: vec![],
            holder_proof_at_s: None,
            scenario: AccessScenario::Nominal,
            random_source: Box::new(OsRandomSource),
        }
    }

    /// Replaces the operating-system random source with a platform adapter.
    ///
    /// The source must provide unpredictable bytes in flight deployments.
    /// Deterministic sources are suitable only for conformance tests.
    pub fn with_random_source(mut self, random_source: impl RandomSource + 'static) -> Self {
        self.random_source = Box::new(random_source);
        self
    }

    pub fn enable_persistent_state(
        &mut self,
        state_dir: impl AsRef<Path>,
    ) -> Result<(), SessionError> {
        let state_dir = state_dir.as_ref();
        self.station_replay = ReplayCache::persistent(state_dir.join("station-nonces.log"))?;
        self.chaser_replay = ReplayCache::persistent(state_dir.join("chaser-nonces.log"))?;
        self.gate_replay = ReplayCache::persistent(state_dir.join("gate-nonces.log"))?;
        self.consumed_grants = ReplayCache::persistent(state_dir.join("consumed-grants.log"))?;
        Ok(())
    }

    /// Replaces all replay domains with caller-configured caches.
    ///
    /// Production adapters should construct each cache with an independent,
    /// rollback-resistant `ReplayStateBackend`. Keeping the domains separate
    /// prevents one protocol role from consuming another role's identifiers.
    pub fn set_replay_state(
        &mut self,
        station_replay: ReplayCache,
        chaser_replay: ReplayCache,
        gate_replay: ReplayCache,
        consumed_grants: ReplayCache,
    ) {
        self.station_replay = station_replay;
        self.chaser_replay = chaser_replay;
        self.gate_replay = gate_replay;
        self.consumed_grants = consumed_grants;
    }

    pub fn protocol_profile(&self) -> &ProtocolProfile {
        &self.config.protocol_profile
    }

    pub fn authorization_policy(&self) -> &crate::AccessPolicyMetadata {
        self.config.authorization_policy_engine.metadata()
    }

    pub fn establish_session(
        &mut self,
        now_s: i64,
        scenario: AccessScenario,
    ) -> Result<SessionOutcome, SessionError> {
        self.reset();
        self.scenario = scenario;
        let session_id = format!("access-{}", hex::encode(self.random_nonce()?));
        let request = exchange(
            ProtocolClaims {
                message_type: MessageType::AccessRequest,
                issuer: self.chaser.key_id().into(),
                recipient: self.station.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
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
            },
            &self.chaser,
            &self.station_peer_trust,
            self.station.key_id(),
            &mut self.station_replay,
            now_s,
        )?;
        self.authorization.identity_verified = true;

        let challenge = self.random_nonce()?;
        let offer = exchange(
            ProtocolClaims {
                message_type: MessageType::SessionOffer,
                issuer: self.station.key_id().into(),
                recipient: self.chaser.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
                session_id: Some(session_id.clone()),
                authorized_stage: None,
                challenge_nonce: Some(challenge.clone()),
                credentials: vec![],
                grant_id: None,
                expires_at_s: None,
                protocol_profile_id: None,
                protocol_profile_version: None,
                rule_id: None,
                authorization_policy_bundle_id: None,
                authorization_policy_bundle_version: None,
                authorization_policy_sha256: None,
            },
            &self.station,
            &self.chaser_peer_trust,
            self.chaser.key_id(),
            &mut self.chaser_replay,
            now_s,
        )?;

        let credential_claims = [
            CredentialClaims {
                issuer: self.credential_issuer.key_id().into(),
                subject: self.chaser.key_id().into(),
                profile_id: "registered-vehicle-v1".into(),
                credential_type: "VehicleRegistrationCredential".into(),
                schema_id: "space:vehicle-registration:v1".into(),
                issuer_group: "recognized-registrars".into(),
                issued_at_s: now_s - 60,
                expires_at_s: if scenario == AccessScenario::ExpiredCredential {
                    now_s - 1
                } else {
                    now_s + 86_400
                },
                status_checked_at_s: now_s,
            },
            CredentialClaims {
                issuer: self.credential_issuer.key_id().into(),
                subject: self.chaser.key_id().into(),
                profile_id: "idss-compatible-v1".into(),
                credential_type: "DockingCertificationCredential".into(),
                schema_id: "space:docking-certification:v1".into(),
                issuer_group: "recognized-docking-authorities".into(),
                issued_at_s: now_s - 60,
                expires_at_s: now_s + 86_400,
                status_checked_at_s: now_s,
            },
        ];
        let credentials = credential_claims
            .iter()
            .map(|claims| issue_credential(claims, &self.credential_issuer))
            .collect::<Result<Vec<_>, _>>()?;
        let proof = exchange(
            ProtocolClaims {
                message_type: MessageType::SessionProof,
                issuer: self.chaser.key_id().into(),
                recipient: self.station.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
                session_id: Some(session_id.clone()),
                authorized_stage: None,
                challenge_nonce: Some(challenge),
                credentials,
                grant_id: None,
                expires_at_s: None,
                protocol_profile_id: None,
                protocol_profile_version: None,
                rule_id: None,
                authorization_policy_bundle_id: None,
                authorization_policy_bundle_version: None,
                authorization_policy_sha256: None,
            },
            &self.chaser,
            &self.station_peer_trust,
            self.station.key_id(),
            &mut self.station_replay,
            now_s,
        )?;
        if proof.claims.session_id != offer.claims.session_id
            || proof.claims.challenge_nonce != offer.claims.challenge_nonce
        {
            return Err(SessionError::SessionBinding);
        }
        if proof.claims.credentials.is_empty() {
            return Err(SessionError::CredentialMissing);
        }
        self.verified_credentials = proof
            .claims
            .credentials
            .iter()
            .map(|credential| {
                let claims =
                    verify_credential(credential, &self.credential_trust, &proof.signer, now_s)?;
                Ok(VerifiedCredentialEvidence {
                    profile_id: claims.profile_id,
                    credential_type: claims.credential_type,
                    schema_id: claims.schema_id,
                    issuer_group: claims.issuer_group,
                    status_checked_at_s: claims.status_checked_at_s,
                })
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        self.config.protocol_profile.validate_foundation(
            now_s,
            &self.config.trust_bundle_id,
            self.config.trust_bundle_version,
            self.config.trust_bundle_issued_at_s,
        )?;
        self.config
            .protocol_profile
            .validate_credentials(&self.verified_credentials, now_s)?;
        self.holder_proof_at_s = Some(now_s);
        let authorization_decision = self.config.authorization_policy_engine.authorize_session(
            self.chaser.key_id(),
            self.station.key_id(),
            &self.verified_credentials,
            true,
        )?;
        self.authorization.session_authorized = true;
        self.session_id = Some(session_id.clone());

        let events = vec![
            event(
                "TRUST_BUNDLE_LOADED",
                "Configured peer and issuer keys loaded",
                None,
                None,
                None,
            ),
            event(
                "ACCESS_REQUEST_VERIFIED",
                "Signed ACCESS request verified with replay protection",
                Some(&request.signer),
                Some(self.station.key_id()),
                Some("ACCESS_REQUEST"),
            ),
            event(
                "SESSION_OFFER_VERIFIED",
                &format!("Signed challenge bound to session={session_id}"),
                Some(self.station.key_id()),
                Some(self.chaser.key_id()),
                Some("SESSION_OFFER"),
            ),
            event(
                "CREDENTIAL_ISSUED",
                &format!(
                    "issuer={}; type={}",
                    credential_claims[0].issuer,
                    "VehicleRegistrationCredential,DockingCertificationCredential"
                ),
                Some(self.credential_issuer.key_id()),
                Some(self.chaser.key_id()),
                Some("CREDENTIAL"),
            ),
            event(
                "CREDENTIALS_VERIFIED",
                "Issuer signature, validity, and holder binding verified",
                Some(self.chaser.key_id()),
                Some(self.station.key_id()),
                Some("CREDENTIAL_PRESENTATION"),
            ),
            event(
                "HOLDER_PROOF_VERIFIED",
                "Fresh station challenge signed by credential subject",
                Some(self.chaser.key_id()),
                Some(self.station.key_id()),
                Some("SESSION_PROOF"),
            ),
            event(
                "ACCESS_INITIAL_CLAIMS_ALLOWED",
                &format!(
                    "bundle={}@{}; hash={}",
                    authorization_decision.policy.bundle_id,
                    authorization_decision.policy.bundle_version,
                    authorization_decision.policy.policy_sha256
                ),
                Some(self.station.key_id()),
                Some(self.chaser.key_id()),
                Some("ACCESS_AUTHORIZATION"),
            ),
            event(
                "SESSION_AUTHORIZED",
                &format!("session={session_id}; replay caches active"),
                Some(self.station.key_id()),
                Some(self.chaser.key_id()),
                Some("SESSION_AUTHORIZATION"),
            ),
        ];
        Ok(SessionOutcome {
            session_id,
            protocol_profile_id: self.config.protocol_profile.profile_id.clone(),
            protocol_profile_version: self.config.protocol_profile.profile_version,
            authorization_decision,
            events,
        })
    }

    pub fn request_transition(
        &mut self,
        requested_state: u8,
        now_s: i64,
        readiness: &ReadinessEvidence,
    ) -> Result<TransitionOutcome, SessionError> {
        let previous = state_number(self.state);
        let (event_kind, stage) = transition_for(self.state, requested_state)
            .ok_or(TransitionError::InvalidTransition)?;
        let session_id = self
            .session_id
            .clone()
            .ok_or(SessionError::SessionBinding)?;
        let challenge = self.random_nonce()?;
        let offer = exchange(
            ProtocolClaims {
                message_type: MessageType::SessionOffer,
                issuer: self.station.key_id().into(),
                recipient: self.chaser.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
                session_id: Some(session_id.clone()),
                authorized_stage: None,
                challenge_nonce: Some(challenge.clone()),
                credentials: vec![],
                grant_id: None,
                expires_at_s: None,
                protocol_profile_id: None,
                protocol_profile_version: None,
                rule_id: None,
                authorization_policy_bundle_id: None,
                authorization_policy_bundle_version: None,
                authorization_policy_sha256: None,
            },
            &self.station,
            &self.chaser_peer_trust,
            self.chaser.key_id(),
            &mut self.chaser_replay,
            now_s,
        )?;
        let proof = exchange(
            ProtocolClaims {
                message_type: MessageType::SessionProof,
                issuer: self.chaser.key_id().into(),
                recipient: self.station.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
                session_id: Some(session_id.clone()),
                authorized_stage: None,
                challenge_nonce: Some(challenge),
                credentials: vec![],
                grant_id: None,
                expires_at_s: None,
                protocol_profile_id: None,
                protocol_profile_version: None,
                rule_id: None,
                authorization_policy_bundle_id: None,
                authorization_policy_bundle_version: None,
                authorization_policy_sha256: None,
            },
            &self.chaser,
            &self.station_peer_trust,
            self.station.key_id(),
            &mut self.station_replay,
            now_s,
        )?;
        if proof.claims.session_id != offer.claims.session_id
            || proof.claims.challenge_nonce != offer.claims.challenge_nonce
        {
            return Err(SessionError::SessionBinding);
        }
        self.holder_proof_at_s = Some(now_s);
        let proof_event = event(
            "HOLDER_PROOF_REFRESHED",
            &format!("Fresh transition proof bound to session={session_id}"),
            Some(self.chaser.key_id()),
            Some(self.station.key_id()),
            Some("SESSION_PROOF"),
        );
        let (from_stage, to_stage) = transition_names(self.state, requested_state)
            .ok_or(TransitionError::InvalidTransition)?;
        let matched_rule_id = self
            .config
            .protocol_profile
            .stage_rules
            .iter()
            .find(|rule| rule.from_stage == from_stage && rule.to_stage == to_stage)
            .map(|rule| rule.rule_id.clone());
        let rule = match self.config.protocol_profile.evaluate(
            from_stage,
            to_stage,
            now_s,
            &self.config.trust_bundle_id,
            self.config.trust_bundle_version,
            self.config.trust_bundle_issued_at_s,
            &self.verified_credentials,
            self.holder_proof_at_s,
            self.authorization.session_authorized,
            readiness,
        ) {
            Ok(rule) => rule,
            Err(error) => {
                let mut outcome = policy_denied(previous, error);
                outcome.session_id = Some(session_id.clone());
                outcome.protocol_profile_id = Some(self.config.protocol_profile.profile_id.clone());
                outcome.protocol_profile_version =
                    Some(self.config.protocol_profile.profile_version);
                outcome.rule_id = matched_rule_id;
                outcome.events.insert(0, proof_event);
                return Ok(outcome);
            }
        };
        let readiness_fresh = now_s
            .saturating_mul(1000)
            .saturating_sub(readiness.observed_at_ms)
            <= self
                .config
                .protocol_profile
                .stage_rules
                .iter()
                .find(|candidate| candidate.rule_id == rule.rule_id)
                .map(|candidate| candidate.readiness.maximum_age_ms)
                .unwrap_or(0);
        let authorization_decision = match self
            .config
            .authorization_policy_engine
            .authorize_transition(
                self.chaser.key_id(),
                self.station.key_id(),
                self.config
                    .protocol_profile
                    .stage_rules
                    .iter()
                    .find(|candidate| candidate.rule_id == rule.rule_id)
                    .map(|candidate| candidate.action.as_str())
                    .unwrap_or_default(),
                &self.verified_credentials,
                self.holder_proof_at_s.is_some(),
                self.authorization.session_authorized,
                readiness,
                readiness_fresh,
            ) {
            Ok(decision) => decision,
            Err(error) => {
                let mut outcome = denied(previous, "DENY_AUTHORIZATION_POLICY", &error.to_string());
                outcome.session_id = Some(session_id.clone());
                outcome.protocol_profile_id = Some(rule.profile_id.clone());
                outcome.protocol_profile_version = Some(rule.profile_version);
                outcome.rule_id = Some(rule.rule_id.clone());
                outcome.events.insert(0, proof_event);
                return Ok(outcome);
            }
        };
        let grant_id = format!("grant-{}", hex::encode(self.random_nonce()?));
        let expires_at_s = now_s.saturating_add(rule.entitlement_ttl_s as i64);
        let encoded = sign_envelope(
            &ProtocolClaims {
                message_type: MessageType::AuthorizationGrant,
                issuer: self.station.key_id().into(),
                recipient: self.chaser.key_id().into(),
                issued_at_s: now_s,
                nonce: self.random_nonce()?,
                session_id: Some(session_id.clone()),
                authorized_stage: Some(stage),
                challenge_nonce: None,
                credentials: vec![],
                grant_id: Some(grant_id.clone()),
                expires_at_s: Some(expires_at_s),
                protocol_profile_id: Some(rule.profile_id.clone()),
                protocol_profile_version: Some(rule.profile_version),
                rule_id: Some(rule.rule_id.clone()),
                authorization_policy_bundle_id: Some(
                    authorization_decision.policy.bundle_id.clone(),
                ),
                authorization_policy_bundle_version: Some(
                    authorization_decision.policy.bundle_version,
                ),
                authorization_policy_sha256: Some(
                    authorization_decision.policy.policy_sha256.clone(),
                ),
            },
            &self.station,
        )?;
        let grant = verify_envelope(
            &encoded,
            &self.gate_trust,
            self.chaser.key_id(),
            &mut self.gate_replay,
            now_s,
            MAX_CLOCK_SKEW_S,
        )?;
        if grant.claims.session_id.as_deref() != Some(session_id.as_str())
            || grant.claims.authorized_stage != Some(stage)
            || grant.claims.grant_id.as_deref() != Some(grant_id.as_str())
            || grant
                .claims
                .expires_at_s
                .is_none_or(|expiry| now_s > expiry)
            || grant.claims.protocol_profile_id.as_deref() != Some(rule.profile_id.as_str())
            || grant.claims.protocol_profile_version != Some(rule.profile_version)
            || grant.claims.rule_id.as_deref() != Some(rule.rule_id.as_str())
            || grant.claims.authorization_policy_bundle_id.as_deref()
                != Some(authorization_decision.policy.bundle_id.as_str())
            || grant.claims.authorization_policy_bundle_version
                != Some(authorization_decision.policy.bundle_version)
            || grant.claims.authorization_policy_sha256.as_deref()
                != Some(authorization_decision.policy.policy_sha256.as_str())
        {
            return Err(SessionError::GrantMismatch);
        }
        self.consumed_grants.consume(grant_id.as_bytes())?;
        let transition_authorization = AuthorizationContext {
            identity_verified: self.authorization.identity_verified,
            session_authorized: self.authorization.session_authorized,
            docking_authorized: stage == AuthorizedStage::HardDock,
        };
        self.state = reduce(self.state, event_kind, transition_authorization)?;
        Ok(TransitionOutcome {
            approved: true,
            previous_state: previous,
            resulting_state: state_number(self.state),
            reason: "ALLOW_VERIFIED_ACCESS_GRANT".into(),
            session_id: Some(session_id.clone()),
            protocol_profile_id: Some(rule.profile_id.clone()),
            protocol_profile_version: Some(rule.profile_version),
            rule_id: Some(rule.rule_id.clone()),
            grant_id: Some(grant_id.clone()),
            entitlement_ttl_s: Some(rule.entitlement_ttl_s),
            grant_expires_at_s: Some(expires_at_s),
            signed_grant_hex: Some(hex::encode(&encoded)),
            authorization_decision: Some(authorization_decision.clone()),
            events: vec![
                proof_event,
                event(
                    "ACCESS_STAGE_POLICY_ALLOWED",
                    &format!(
                        "bundle={}@{}; rule={}",
                        authorization_decision.policy.bundle_id,
                        authorization_decision.policy.bundle_version,
                        rule.rule_id
                    ),
                    Some(self.station.key_id()),
                    Some(self.chaser.key_id()),
                    Some("ACCESS_AUTHORIZATION"),
                ),
                event(
                    "AUTHORIZATION_GRANT_ISSUED",
                    &format!(
                        "grant={grant_id}; session={session_id}; rule={}; stage={stage:?}; ttl={}s",
                        rule.rule_id, rule.entitlement_ttl_s
                    ),
                    Some(self.station.key_id()),
                    Some(self.chaser.key_id()),
                    Some("AUTHORIZATION_GRANT"),
                ),
                event(
                    "AUTHORIZATION_GRANT_CONSUMED",
                    "Signature, audience, session, stage, freshness, and nonce verified",
                    Some(self.station.key_id()),
                    Some("transition-gate"),
                    Some("GRANT_CONSUMPTION"),
                ),
            ],
        })
    }

    fn reset(&mut self) {
        self.station_replay.reset_ephemeral();
        self.chaser_replay.reset_ephemeral();
        self.gate_replay.reset_ephemeral();
        self.consumed_grants.reset_ephemeral();
        self.state = DockingState::Hold;
        self.authorization = AuthorizationContext::default();
        self.session_id = None;
        self.verified_credentials.clear();
        self.holder_proof_at_s = None;
    }

    fn random_nonce(&self) -> Result<Vec<u8>, SessionError> {
        let mut nonce = vec![0_u8; 32];
        self.random_source
            .fill(&mut nonce)
            .map_err(|_| SessionError::RandomSource)?;
        Ok(nonce)
    }
}

fn exchange(
    claims: ProtocolClaims,
    signer: &IdentityKey,
    trust: &TrustStore,
    recipient: &str,
    replay: &mut ReplayCache,
    now_s: i64,
) -> Result<VerifiedEnvelope, ProtocolError> {
    let encoded = sign_envelope(&claims, signer)?;
    verify_envelope(&encoded, trust, recipient, replay, now_s, MAX_CLOCK_SKEW_S)
}

fn state_number(state: DockingState) -> u8 {
    match state {
        DockingState::Hold => 0,
        DockingState::Approach => 1,
        DockingState::FinalApproach => 2,
        DockingState::SoftCapture => 3,
        DockingState::HardDock => 4,
        DockingState::Aborted => 5,
    }
}

fn transition_for(state: DockingState, requested: u8) -> Option<(DockingEvent, AuthorizedStage)> {
    match (state, requested) {
        (DockingState::Hold, 1) => Some((DockingEvent::BeginApproach, AuthorizedStage::Approach)),
        (DockingState::Approach, 2) => Some((
            DockingEvent::EnterFinalApproach,
            AuthorizedStage::FinalApproach,
        )),
        (DockingState::FinalApproach, 3) => Some((
            DockingEvent::EngageSoftCapture,
            AuthorizedStage::SoftCapture,
        )),
        (DockingState::SoftCapture, 4) => {
            Some((DockingEvent::EngageHardDock, AuthorizedStage::HardDock))
        }
        _ => None,
    }
}

fn transition_names(state: DockingState, requested: u8) -> Option<(&'static str, &'static str)> {
    match (state, requested) {
        (DockingState::Hold, 1) => Some(("hold", "approach")),
        (DockingState::Approach, 2) => Some(("approach", "final_approach")),
        (DockingState::FinalApproach, 3) => Some(("final_approach", "soft_capture")),
        (DockingState::SoftCapture, 4) => Some(("soft_capture", "hard_dock")),
        _ => None,
    }
}

fn policy_denied(previous: u8, error: ProtocolProfileError) -> TransitionOutcome {
    let reason = match error {
        ProtocolProfileError::ProfileExpired => "DENY_PROTOCOL_PROFILE_EXPIRED",
        ProtocolProfileError::TrustBundleMismatch => "DENY_TRUST_BUNDLE",
        ProtocolProfileError::NoMatchingRule => "DENY_NO_MATCHING_PROTOCOL_RULE",
        ProtocolProfileError::CredentialRequired => "DENY_CREDENTIAL_REQUIRED",
        ProtocolProfileError::HolderProofRequired => "DENY_HOLDER_PROOF",
        ProtocolProfileError::SessionRequired => "DENY_SESSION_NOT_AUTHORIZED",
        ProtocolProfileError::ReadinessStale => "DENY_READINESS_STALE",
        ProtocolProfileError::InvalidTimestamp => "DENY_PROTOCOL_PROFILE_INVALID",
    };
    denied(previous, reason, &error.to_string())
}

fn denied(previous: u8, reason: &str, detail: &str) -> TransitionOutcome {
    TransitionOutcome {
        approved: false,
        previous_state: previous,
        resulting_state: previous,
        reason: reason.into(),
        session_id: None,
        protocol_profile_id: None,
        protocol_profile_version: None,
        rule_id: None,
        grant_id: None,
        entitlement_ttl_s: None,
        grant_expires_at_s: None,
        signed_grant_hex: None,
        authorization_decision: None,
        events: vec![event(reason, detail, None, None, None)],
    }
}

fn event(
    code: &str,
    detail: &str,
    from: Option<&str>,
    to: Option<&str>,
    message_type: Option<&str>,
) -> AccessEvent {
    AccessEvent {
        code: code.into(),
        detail: detail.into(),
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
        message_type: message_type.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_S: i64 = 1_787_900_100;

    struct FailingRandomSource;

    impl RandomSource for FailingRandomSource {
        fn fill(&self, _destination: &mut [u8]) -> Result<(), String> {
            Err("qualified random source unavailable".into())
        }
    }

    fn engine() -> AccessEngine {
        let chaser = IdentityKey::from_seed("odyssey-7", [1; 32]);
        let station = IdentityKey::from_seed("waystation-1", [2; 32]);
        let issuer = IdentityKey::from_seed("orbital-registry", [3; 32]);
        let mut station_peer_trust = TrustStore::default();
        station_peer_trust.insert(chaser.key_id(), chaser.verifying_key());
        let mut chaser_peer_trust = TrustStore::default();
        chaser_peer_trust.insert(station.key_id(), station.verifying_key());
        let mut credential_trust = TrustStore::default();
        credential_trust.insert(issuer.key_id(), issuer.verifying_key());
        let mut gate_trust = TrustStore::default();
        gate_trust.insert(station.key_id(), station.verifying_key());
        AccessEngine::new(
            chaser,
            station,
            issuer,
            station_peer_trust,
            chaser_peer_trust,
            credential_trust,
            gate_trust,
            AccessEngineConfig {
                protocol_profile: ProtocolProfile::from_json(include_bytes!(
                    "../../../config/access/access-protocol-profile.json"
                ))
                .unwrap(),
                authorization_policy_engine: CedarPolicyEngine::from_source(
                    "waystation-1-commercial-authorization",
                    1,
                    include_str!(
                        "../../../examples/authorization/policies/commercial-docking.cedar"
                    ),
                )
                .unwrap(),
                trust_bundle_id: "waystation-1-trust".into(),
                trust_bundle_version: 42,
                trust_bundle_issued_at_s: NOW_S - 60,
            },
        )
    }

    fn readiness(requested_state: u8, now_s: i64) -> ReadinessEvidence {
        let checks = [
            "initial_hold_confirmed",
            "retreat_available",
            "relative_navigation_valid",
            "approach_corridor_clear",
            "closing_rate_within_limit",
            "alignment_within_limit",
            "capture_system_ready",
            "soft_capture_confirmed",
            "latches_ready",
            "relative_motion_stable",
        ]
        .into_iter()
        .map(|check| (check.into(), true))
        .collect();
        ReadinessEvidence {
            observed_at_ms: now_s * 1000,
            range_m: match requested_state {
                1 => 3.32,
                2 => 1.12,
                3 => 0.32,
                4 => 0.04,
                _ => f64::MAX,
            },
            closing_rate_mps: 0.01,
            checks,
        }
    }

    #[test]
    fn cryptographic_session_drives_all_protected_transitions() {
        let mut engine = engine();
        let events = engine
            .establish_session(NOW_S, AccessScenario::Nominal)
            .unwrap();
        assert!(
            events
                .events
                .iter()
                .any(|event| event.code == "CREDENTIALS_VERIFIED")
        );
        for requested in 1..=4 {
            let now_s = NOW_S + requested as i64;
            assert!(
                engine
                    .request_transition(requested, now_s, &readiness(requested, now_s))
                    .unwrap()
                    .approved
            );
        }
    }

    #[test]
    fn expired_credential_prevents_session_authorization() {
        let error = engine()
            .establish_session(NOW_S, AccessScenario::ExpiredCredential)
            .unwrap_err();
        assert!(matches!(
            error,
            SessionError::Protocol(ProtocolError::CredentialExpired)
        ));
    }

    #[test]
    fn random_source_failure_prevents_session_authorization() {
        let error = engine()
            .with_random_source(FailingRandomSource)
            .establish_session(NOW_S, AccessScenario::Nominal)
            .unwrap_err();
        assert!(matches!(error, SessionError::RandomSource));
    }

    #[test]
    fn corridor_violation_withholds_final_approach_grant() {
        let mut engine = engine();
        engine
            .establish_session(NOW_S, AccessScenario::Nominal)
            .unwrap();
        assert!(
            engine
                .request_transition(1, NOW_S + 1, &readiness(1, NOW_S + 1))
                .unwrap()
                .approved
        );

        let mut failed = readiness(2, NOW_S + 2);
        failed
            .checks
            .insert("approach_corridor_clear".into(), false);
        let outcome = engine.request_transition(2, NOW_S + 2, &failed).unwrap();
        assert!(!outcome.approved);
        assert_eq!(outcome.reason, "DENY_AUTHORIZATION_POLICY");
        assert_eq!(outcome.resulting_state, 1);
    }

    #[test]
    fn incomplete_latches_withhold_hard_dock_grant() {
        let mut engine = engine();
        engine
            .establish_session(NOW_S, AccessScenario::Nominal)
            .unwrap();
        for requested in 1..=3 {
            let now_s = NOW_S + requested as i64;
            assert!(
                engine
                    .request_transition(requested, now_s, &readiness(requested, now_s))
                    .unwrap()
                    .approved
            );
        }

        let mut failed = readiness(4, NOW_S + 4);
        failed.checks.insert("latches_ready".into(), false);
        let outcome = engine.request_transition(4, NOW_S + 4, &failed).unwrap();
        assert!(!outcome.approved);
        assert_eq!(outcome.reason, "DENY_AUTHORIZATION_POLICY");
        assert_eq!(outcome.resulting_state, 3);
    }
}
