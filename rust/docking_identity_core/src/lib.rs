//! Security-critical docking protocol and deterministic transition logic.

mod policy;
mod protocol;
mod session;
mod state_machine;

pub use policy::{
    AuthorizationPolicy, PolicyError, ReadinessEvidence, RuleDecision, VerifiedCredentialEvidence,
};
pub use protocol::{
    AuthorizedStage, CredentialClaims, IdentityKey, MessageType, PayloadSigner, ProtocolClaims,
    ProtocolError, ReplayCache, TrustStore, VerifiedEnvelope, issue_credential, sign_envelope,
    verify_credential, verify_envelope,
};
pub use session::{
    AccessEngine, AccessEngineConfig, AccessEvent, AccessScenario, SessionError, SessionOutcome,
    TransitionOutcome,
};
pub use state_machine::{
    AuthorizationContext, DockingEvent, DockingState, TransitionError, reduce,
};
