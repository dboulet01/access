//! Security-critical docking protocol and deterministic transition logic.

mod protocol;
mod state_machine;

pub use protocol::{
    AuthorizedStage, IdentityKey, MessageType, ProtocolClaims, ProtocolError, ReplayCache,
    TrustStore, VerifiedEnvelope, sign_envelope, verify_envelope,
};
pub use state_machine::{
    AuthorizationContext, DockingEvent, DockingState, TransitionError, reduce,
};
