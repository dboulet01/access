//! Portable ACCESS protocol, authorization, entitlement, and enforcement core.

mod cedar;
mod protocol;
mod protocol_profile;
mod session;
mod state_machine;

pub use cedar::{AccessDecision, AccessPolicyError, AccessPolicyMetadata, CedarPolicyEngine};
pub use protocol::{
    AuthorizedStage, CredentialClaims, IdentityKey, MessageType, PayloadSigner, ProtocolClaims,
    ProtocolError, ReplayCache, ReplayStateBackend, TrustStore, VerifiedEnvelope, issue_credential,
    sign_envelope, verify_credential, verify_envelope,
};
pub use protocol_profile::{
    ProtocolProfile, ProtocolProfileError, ProtocolRuleDecision, ReadinessEvidence,
    VerifiedCredentialEvidence,
};
pub use session::{
    AccessEngine, AccessEngineConfig, AccessEvent, AccessScenario, OsRandomSource, RandomSource,
    SessionError, SessionOutcome, TransitionOutcome,
};
pub use state_machine::{
    AuthorizationContext, DockingEvent, DockingState, TransitionError, reduce,
};
