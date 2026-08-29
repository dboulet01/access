use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DockingState {
    #[default]
    Hold,
    Approach,
    FinalApproach,
    SoftCapture,
    HardDock,
    Aborted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockingEvent {
    BeginApproach,
    EnterFinalApproach,
    EngageSoftCapture,
    EngageHardDock,
    Abort,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthorizationContext {
    pub identity_verified: bool,
    pub session_authorized: bool,
    pub docking_authorized: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TransitionError {
    #[error("event is invalid from the current docking state")]
    InvalidTransition,
    #[error("verified identity is required for final approach")]
    IdentityRequired,
    #[error("authorized session is required for soft capture")]
    SessionRequired,
    #[error("docking authorization is required for hard dock")]
    DockingAuthorizationRequired,
}

pub fn reduce(
    state: DockingState,
    event: DockingEvent,
    authorization: AuthorizationContext,
) -> Result<DockingState, TransitionError> {
    if event == DockingEvent::Abort {
        return Ok(DockingState::Aborted);
    }

    match (state, event) {
        (DockingState::Hold, DockingEvent::BeginApproach) => Ok(DockingState::Approach),
        (DockingState::Approach, DockingEvent::EnterFinalApproach) => authorization
            .identity_verified
            .then_some(DockingState::FinalApproach)
            .ok_or(TransitionError::IdentityRequired),
        (DockingState::FinalApproach, DockingEvent::EngageSoftCapture) => authorization
            .session_authorized
            .then_some(DockingState::SoftCapture)
            .ok_or(TransitionError::SessionRequired),
        (DockingState::SoftCapture, DockingEvent::EngageHardDock) => authorization
            .docking_authorized
            .then_some(DockingState::HardDock)
            .ok_or(TransitionError::DockingAuthorizationRequired),
        _ => Err(TransitionError::InvalidTransition),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_transitions_fail_closed() {
        let denied = AuthorizationContext::default();

        assert_eq!(
            reduce(
                DockingState::Approach,
                DockingEvent::EnterFinalApproach,
                denied,
            ),
            Err(TransitionError::IdentityRequired)
        );
        assert_eq!(
            reduce(
                DockingState::FinalApproach,
                DockingEvent::EngageSoftCapture,
                denied,
            ),
            Err(TransitionError::SessionRequired)
        );
        assert_eq!(
            reduce(
                DockingState::SoftCapture,
                DockingEvent::EngageHardDock,
                denied,
            ),
            Err(TransitionError::DockingAuthorizationRequired)
        );
    }

    #[test]
    fn authorized_sequence_reaches_hard_dock() {
        let authorized = AuthorizationContext {
            identity_verified: true,
            session_authorized: true,
            docking_authorized: true,
        };
        let mut state = DockingState::Hold;
        for event in [
            DockingEvent::BeginApproach,
            DockingEvent::EnterFinalApproach,
            DockingEvent::EngageSoftCapture,
            DockingEvent::EngageHardDock,
        ] {
            state = reduce(state, event, authorized).unwrap();
        }
        assert_eq!(state, DockingState::HardDock);
    }
}
