use std::collections::{HashMap, HashSet};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthorizationPolicy {
    pub policy_id: String,
    pub policy_version: u64,
    pub valid_from: String,
    pub valid_until: String,
    pub trust_bundle: PolicyTrustBundle,
    pub credential_profiles: Vec<CredentialProfile>,
    pub stage_policies: Vec<StagePolicy>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PolicyTrustBundle {
    pub bundle_id: String,
    pub minimum_version: u64,
    pub maximum_age_s: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CredentialProfile {
    pub profile_id: String,
    pub credential_type: String,
    pub schema_id: String,
    pub issuer_groups: Vec<String>,
    pub maximum_status_age_s: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StagePolicy {
    pub rule_id: String,
    pub action: String,
    pub from_stage: String,
    pub to_stage: String,
    pub required_credential_profiles: Vec<String>,
    pub holder_proof_required: bool,
    pub required_session_status: String,
    pub maximum_proof_age_s: Option<i64>,
    pub readiness: ReadinessRule,
    pub entitlement_ttl_s: u64,
    #[serde(default)]
    pub constraints: Constraints,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReadinessRule {
    pub maximum_age_ms: i64,
    pub required_checks: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Constraints {
    pub max_closing_rate_mps: Option<f64>,
    pub max_range_m: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReadinessEvidence {
    pub observed_at_ms: i64,
    pub range_m: f64,
    pub closing_rate_mps: f64,
    pub checks: HashMap<String, bool>,
}

#[derive(Clone, Debug)]
pub struct VerifiedCredentialEvidence {
    pub profile_id: String,
    pub credential_type: String,
    pub schema_id: String,
    pub issuer_group: String,
    pub status_checked_at_s: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleDecision {
    pub policy_id: String,
    pub policy_version: u64,
    pub rule_id: String,
    pub entitlement_ttl_s: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PolicyError {
    #[error("authorization policy is outside its validity interval")]
    PolicyExpired,
    #[error("configured trust bundle does not satisfy policy")]
    TrustBundleMismatch,
    #[error("no policy rule matches this transition")]
    NoMatchingRule,
    #[error("required credential profile is missing or invalid")]
    CredentialRequired,
    #[error("fresh holder proof is required")]
    HolderProofRequired,
    #[error("authorized session is required")]
    SessionRequired,
    #[error("readiness evidence is stale")]
    ReadinessStale,
    #[error("required readiness check failed: {0}")]
    ReadinessFailed(String),
    #[error("operational constraint failed")]
    ConstraintFailed,
    #[error("policy timestamp is invalid")]
    InvalidTimestamp,
}

impl AuthorizationPolicy {
    pub fn from_json(encoded: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(encoded)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &self,
        from_stage: &str,
        to_stage: &str,
        now_s: i64,
        trust_bundle_id: &str,
        trust_bundle_version: u64,
        trust_bundle_issued_at_s: i64,
        credentials: &[VerifiedCredentialEvidence],
        holder_proof_at_s: Option<i64>,
        session_authorized: bool,
        readiness: &ReadinessEvidence,
    ) -> Result<RuleDecision, PolicyError> {
        let valid_from = parse_timestamp(&self.valid_from)?;
        let valid_until = parse_timestamp(&self.valid_until)?;
        if now_s < valid_from || now_s > valid_until {
            return Err(PolicyError::PolicyExpired);
        }
        if self.trust_bundle.bundle_id != trust_bundle_id
            || trust_bundle_version < self.trust_bundle.minimum_version
            || now_s.saturating_sub(trust_bundle_issued_at_s) > self.trust_bundle.maximum_age_s
        {
            return Err(PolicyError::TrustBundleMismatch);
        }

        let rule = self
            .stage_policies
            .iter()
            .find(|rule| rule.from_stage == from_stage && rule.to_stage == to_stage)
            .ok_or(PolicyError::NoMatchingRule)?;
        self.verify_credentials(rule, credentials, now_s)?;
        if rule.holder_proof_required
            && holder_proof_at_s.is_none_or(|proof_at| {
                now_s.saturating_sub(proof_at) > rule.maximum_proof_age_s.unwrap_or(0)
            })
        {
            return Err(PolicyError::HolderProofRequired);
        }
        if rule.required_session_status == "authorized" && !session_authorized {
            return Err(PolicyError::SessionRequired);
        }
        if now_s
            .saturating_mul(1000)
            .saturating_sub(readiness.observed_at_ms)
            > rule.readiness.maximum_age_ms
        {
            return Err(PolicyError::ReadinessStale);
        }
        if let Some(failed_check) = rule
            .readiness
            .required_checks
            .iter()
            .find(|check| !readiness.checks.get(*check).copied().unwrap_or(false))
        {
            return Err(PolicyError::ReadinessFailed(failed_check.clone()));
        }
        if rule
            .constraints
            .max_closing_rate_mps
            .is_some_and(|limit| readiness.closing_rate_mps > limit)
            || rule
                .constraints
                .max_range_m
                .is_some_and(|limit| readiness.range_m > limit)
        {
            return Err(PolicyError::ConstraintFailed);
        }

        Ok(RuleDecision {
            policy_id: self.policy_id.clone(),
            policy_version: self.policy_version,
            rule_id: rule.rule_id.clone(),
            entitlement_ttl_s: rule.entitlement_ttl_s,
        })
    }

    fn verify_credentials(
        &self,
        rule: &StagePolicy,
        credentials: &[VerifiedCredentialEvidence],
        now_s: i64,
    ) -> Result<(), PolicyError> {
        let required: HashSet<_> = rule.required_credential_profiles.iter().collect();
        for profile_id in required {
            let profile = self
                .credential_profiles
                .iter()
                .find(|profile| &profile.profile_id == profile_id)
                .ok_or(PolicyError::CredentialRequired)?;
            let verified = credentials.iter().find(|credential| {
                credential.profile_id == profile.profile_id
                    && credential.credential_type == profile.credential_type
                    && credential.schema_id == profile.schema_id
                    && profile.issuer_groups.contains(&credential.issuer_group)
                    && profile.maximum_status_age_s.is_none_or(|maximum_age| {
                        now_s.saturating_sub(credential.status_checked_at_s) <= maximum_age
                    })
            });
            if verified.is_none() {
                return Err(PolicyError::CredentialRequired);
            }
        }
        Ok(())
    }
}

fn parse_timestamp(value: &str) -> Result<i64, PolicyError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp())
        .map_err(|_| PolicyError::InvalidTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Vec<VerifiedCredentialEvidence> {
        vec![
            VerifiedCredentialEvidence {
                profile_id: "registered-vehicle-v1".into(),
                credential_type: "VehicleRegistrationCredential".into(),
                schema_id: "space:vehicle-registration:v1".into(),
                issuer_group: "recognized-registrars".into(),
                status_checked_at_s: 1_787_900_000,
            },
            VerifiedCredentialEvidence {
                profile_id: "idss-compatible-v1".into(),
                credential_type: "DockingCertificationCredential".into(),
                schema_id: "space:docking-certification:v1".into(),
                issuer_group: "recognized-docking-authorities".into(),
                status_checked_at_s: 1_787_900_000,
            },
        ]
    }

    #[test]
    fn evaluates_repository_policy_against_fresh_readiness() {
        let policy = AuthorizationPolicy::from_json(include_bytes!(
            "../../../examples/authorization/commercial-docking.policy.json"
        ))
        .unwrap();
        let now_s = 1_787_900_100;
        let mut checks = HashMap::new();
        checks.insert("relative_navigation_valid".into(), true);
        checks.insert("approach_corridor_clear".into(), true);
        checks.insert("closing_rate_within_limit".into(), true);
        let readiness = ReadinessEvidence {
            observed_at_ms: now_s * 1000,
            range_m: 1.12,
            closing_rate_mps: 0.18,
            checks,
        };

        let decision = policy
            .evaluate(
                "approach",
                "final_approach",
                now_s,
                "waystation-1-trust",
                42,
                now_s - 60,
                &credentials(),
                Some(now_s - 1),
                true,
                &readiness,
            )
            .unwrap();
        assert_eq!(decision.rule_id, "enter-final-approach");
        assert_eq!(decision.entitlement_ttl_s, 30);

        let mut failed = readiness;
        failed
            .checks
            .insert("approach_corridor_clear".into(), false);
        assert_eq!(
            policy.evaluate(
                "approach",
                "final_approach",
                now_s,
                "waystation-1-trust",
                42,
                now_s - 60,
                &credentials(),
                Some(now_s - 1),
                true,
                &failed,
            ),
            Err(PolicyError::ReadinessFailed(
                "approach_corridor_clear".into()
            ))
        );
    }
}
