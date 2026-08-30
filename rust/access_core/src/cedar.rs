use std::str::FromStr;

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{ReadinessEvidence, VerifiedCredentialEvidence};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccessPolicyMetadata {
    pub bundle_id: String,
    pub bundle_version: u64,
    pub policy_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccessDecision {
    pub gate: String,
    pub decision: String,
    pub diagnostics: String,
    pub policy: AccessPolicyMetadata,
}

#[derive(Debug, Error)]
pub enum AccessPolicyError {
    #[error("ACCESS authorization policy set is invalid: {0}")]
    InvalidPolicy(String),
    #[error("ACCESS authorization request is invalid: {0}")]
    InvalidRequest(String),
    #[error("ACCESS authorization policy denied the request at gate {gate}")]
    Denied { gate: String },
    #[error("ACCESS authorization evaluation failed at gate {gate}: {detail}")]
    Evaluation { gate: String, detail: String },
}

pub struct CedarPolicyEngine {
    authorizer: Authorizer,
    policies: PolicySet,
    metadata: AccessPolicyMetadata,
}

impl CedarPolicyEngine {
    pub fn from_source(
        bundle_id: impl Into<String>,
        bundle_version: u64,
        source: &str,
    ) -> Result<Self, AccessPolicyError> {
        let policies = PolicySet::from_str(source)
            .map_err(|error| AccessPolicyError::InvalidPolicy(error.to_string()))?;
        if policies.is_empty() {
            return Err(AccessPolicyError::InvalidPolicy(
                "policy set must contain at least one policy".into(),
            ));
        }
        let policy_sha256 = format!("sha-256:{}", hex::encode(Sha256::digest(source.as_bytes())));
        Ok(Self {
            authorizer: Authorizer::new(),
            policies,
            metadata: AccessPolicyMetadata {
                bundle_id: bundle_id.into(),
                bundle_version,
                policy_sha256,
            },
        })
    }

    pub fn metadata(&self) -> &AccessPolicyMetadata {
        &self.metadata
    }

    pub fn authorize_session(
        &self,
        vehicle_id: &str,
        station_id: &str,
        credentials: &[VerifiedCredentialEvidence],
        holder_proof_valid: bool,
    ) -> Result<AccessDecision, AccessPolicyError> {
        self.authorize(
            "initial_claims",
            vehicle_id,
            "authorize_session",
            station_id,
            context(credentials, holder_proof_valid, false, None, false),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn authorize_transition(
        &self,
        vehicle_id: &str,
        station_id: &str,
        action: &str,
        credentials: &[VerifiedCredentialEvidence],
        holder_proof_valid: bool,
        session_authorized: bool,
        readiness: &ReadinessEvidence,
        readiness_fresh: bool,
    ) -> Result<AccessDecision, AccessPolicyError> {
        self.authorize(
            "stage_transition",
            vehicle_id,
            action,
            station_id,
            context(
                credentials,
                holder_proof_valid,
                session_authorized,
                Some(readiness),
                readiness_fresh,
            ),
        )
    }

    fn authorize(
        &self,
        gate: &str,
        vehicle_id: &str,
        action: &str,
        station_id: &str,
        context_value: Value,
    ) -> Result<AccessDecision, AccessPolicyError> {
        let principal: EntityUid = format!("Vehicle::{vehicle_id:?}").parse().map_err(
            |error: cedar_policy::ParseErrors| AccessPolicyError::InvalidRequest(error.to_string()),
        )?;
        let action: EntityUid =
            format!("Action::{action:?}")
                .parse()
                .map_err(|error: cedar_policy::ParseErrors| {
                    AccessPolicyError::InvalidRequest(error.to_string())
                })?;
        let resource: EntityUid = format!("Station::{station_id:?}").parse().map_err(
            |error: cedar_policy::ParseErrors| AccessPolicyError::InvalidRequest(error.to_string()),
        )?;
        let context = Context::from_json_value(context_value, None)
            .map_err(|error| AccessPolicyError::InvalidRequest(error.to_string()))?;
        let request = Request::new(principal, action, resource, context, None)
            .map_err(|error| AccessPolicyError::InvalidRequest(error.to_string()))?;
        let response = self
            .authorizer
            .is_authorized(&request, &self.policies, &Entities::empty());
        let diagnostics = format!("{:?}", response.diagnostics());
        if response.diagnostics().errors().next().is_some() {
            return Err(AccessPolicyError::Evaluation {
                gate: gate.into(),
                detail: diagnostics,
            });
        }
        if response.decision() != Decision::Allow {
            return Err(AccessPolicyError::Denied { gate: gate.into() });
        }
        Ok(AccessDecision {
            gate: gate.into(),
            decision: "allow".into(),
            diagnostics,
            policy: self.metadata.clone(),
        })
    }
}

fn context(
    credentials: &[VerifiedCredentialEvidence],
    holder_proof_valid: bool,
    session_authorized: bool,
    readiness: Option<&ReadinessEvidence>,
    readiness_fresh: bool,
) -> Value {
    let has_profile = |profile: &str| {
        credentials
            .iter()
            .any(|credential| credential.profile_id == profile)
    };
    let check = |name: &str| {
        readiness
            .and_then(|value| value.checks.get(name))
            .copied()
            .unwrap_or(false)
    };
    let range_mm = readiness
        .map(|value| (value.range_m * 1000.0).ceil() as i64)
        .unwrap_or(i64::MAX);
    let closing_rate_mmps = readiness
        .map(|value| (value.closing_rate_mps * 1000.0).ceil() as i64)
        .unwrap_or(i64::MAX);

    json!({
        "registered_vehicle": has_profile("registered-vehicle-v1"),
        "docking_certified": has_profile("idss-compatible-v1"),
        "holder_proof_valid": holder_proof_valid,
        "session_authorized": session_authorized,
        "readiness_fresh": readiness_fresh,
        "initial_hold_confirmed": check("initial_hold_confirmed"),
        "retreat_available": check("retreat_available"),
        "relative_navigation_valid": check("relative_navigation_valid"),
        "approach_corridor_clear": check("approach_corridor_clear"),
        "closing_rate_within_limit": check("closing_rate_within_limit"),
        "alignment_within_limit": check("alignment_within_limit"),
        "capture_system_ready": check("capture_system_ready"),
        "soft_capture_confirmed": check("soft_capture_confirmed"),
        "latches_ready": check("latches_ready"),
        "relative_motion_stable": check("relative_motion_stable"),
        "range_mm": range_mm,
        "closing_rate_mmps": closing_rate_mmps
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const POLICY: &str = r#"
        permit(principal, action == Action::"authorize_session", resource)
        when { context.registered_vehicle && context.docking_certified && context.holder_proof_valid };
        permit(principal, action == Action::"enter_approach", resource)
        when { context.registered_vehicle && context.readiness_fresh && context.initial_hold_confirmed && context.closing_rate_mmps <= 600 };
    "#;

    fn credentials() -> Vec<VerifiedCredentialEvidence> {
        ["registered-vehicle-v1", "idss-compatible-v1"]
            .into_iter()
            .map(|profile_id| VerifiedCredentialEvidence {
                profile_id: profile_id.into(),
                credential_type: "type".into(),
                schema_id: "schema".into(),
                issuer_group: "issuer".into(),
                status_checked_at_s: 0,
            })
            .collect()
    }

    #[test]
    fn permits_verified_claims_and_denies_missing_claims() {
        let engine = CedarPolicyEngine::from_source("test", 1, POLICY).unwrap();
        assert!(
            engine
                .authorize_session("vehicle-1", "station-1", &credentials(), true)
                .is_ok()
        );
        assert!(matches!(
            engine.authorize_session("vehicle-1", "station-1", &[], true),
            Err(AccessPolicyError::Denied { .. })
        ));
    }

    #[test]
    fn evaluates_operational_context() {
        let engine = CedarPolicyEngine::from_source("test", 1, POLICY).unwrap();
        let mut checks = HashMap::new();
        checks.insert("initial_hold_confirmed".into(), true);
        let readiness = ReadinessEvidence {
            observed_at_ms: 1_000,
            range_m: 5.0,
            closing_rate_mps: 0.6,
            checks,
        };
        assert!(
            engine
                .authorize_transition(
                    "vehicle-1",
                    "station-1",
                    "enter_approach",
                    &credentials(),
                    true,
                    true,
                    &readiness,
                    true,
                )
                .is_ok()
        );
    }
}
