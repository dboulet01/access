use std::collections::HashMap;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProtocolProfile {
    pub schema_version: String,
    pub profile_id: String,
    pub profile_version: u64,
    pub valid_from: String,
    pub valid_until: String,
    pub trust_bundle: TrustBundleRequirements,
    pub credential_profiles: Vec<CredentialProfile>,
    pub stage_rules: Vec<StageRuleProfile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrustBundleRequirements {
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
pub struct StageRuleProfile {
    pub rule_id: String,
    pub action: String,
    pub from_stage: String,
    pub to_stage: String,
    pub holder_proof_required: bool,
    pub required_session_status: String,
    pub maximum_proof_age_s: Option<i64>,
    pub readiness: ReadinessRule,
    pub entitlement_ttl_s: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReadinessRule {
    pub maximum_age_ms: i64,
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
pub struct ProtocolRuleDecision {
    pub profile_id: String,
    pub profile_version: u64,
    pub rule_id: String,
    pub entitlement_ttl_s: u64,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolProfileError {
    #[error("protocol profile is outside its validity interval")]
    ProfileExpired,
    #[error("configured trust bundle does not satisfy protocol profile")]
    TrustBundleMismatch,
    #[error("no protocol rule matches this transition")]
    NoMatchingRule,
    #[error("required credential profile is missing or invalid")]
    CredentialRequired,
    #[error("fresh holder proof is required")]
    HolderProofRequired,
    #[error("authorized session is required")]
    SessionRequired,
    #[error("readiness evidence is stale")]
    ReadinessStale,
    #[error("protocol profile timestamp is invalid")]
    InvalidTimestamp,
}

impl ProtocolProfile {
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
    ) -> Result<ProtocolRuleDecision, ProtocolProfileError> {
        self.validate_foundation(
            now_s,
            trust_bundle_id,
            trust_bundle_version,
            trust_bundle_issued_at_s,
        )?;
        self.validate_credentials(credentials, now_s)?;

        let rule = self
            .stage_rules
            .iter()
            .find(|rule| rule.from_stage == from_stage && rule.to_stage == to_stage)
            .ok_or(ProtocolProfileError::NoMatchingRule)?;
        if rule.holder_proof_required
            && holder_proof_at_s.is_none_or(|proof_at| {
                now_s.saturating_sub(proof_at) > rule.maximum_proof_age_s.unwrap_or(0)
            })
        {
            return Err(ProtocolProfileError::HolderProofRequired);
        }
        if rule.required_session_status == "authorized" && !session_authorized {
            return Err(ProtocolProfileError::SessionRequired);
        }
        if now_s
            .saturating_mul(1000)
            .saturating_sub(readiness.observed_at_ms)
            > rule.readiness.maximum_age_ms
        {
            return Err(ProtocolProfileError::ReadinessStale);
        }
        Ok(ProtocolRuleDecision {
            profile_id: self.profile_id.clone(),
            profile_version: self.profile_version,
            rule_id: rule.rule_id.clone(),
            entitlement_ttl_s: rule.entitlement_ttl_s,
        })
    }

    pub fn validate_foundation(
        &self,
        now_s: i64,
        trust_bundle_id: &str,
        trust_bundle_version: u64,
        trust_bundle_issued_at_s: i64,
    ) -> Result<(), ProtocolProfileError> {
        let valid_from = parse_timestamp(&self.valid_from)?;
        let valid_until = parse_timestamp(&self.valid_until)?;
        if now_s < valid_from || now_s > valid_until {
            return Err(ProtocolProfileError::ProfileExpired);
        }
        if self.trust_bundle.bundle_id != trust_bundle_id
            || trust_bundle_version < self.trust_bundle.minimum_version
            || now_s.saturating_sub(trust_bundle_issued_at_s) > self.trust_bundle.maximum_age_s
        {
            return Err(ProtocolProfileError::TrustBundleMismatch);
        }
        Ok(())
    }

    pub fn validate_credentials(
        &self,
        credentials: &[VerifiedCredentialEvidence],
        now_s: i64,
    ) -> Result<(), ProtocolProfileError> {
        for credential in credentials {
            let profile = self
                .credential_profiles
                .iter()
                .find(|profile| profile.profile_id == credential.profile_id)
                .ok_or(ProtocolProfileError::CredentialRequired)?;
            if credential.credential_type != profile.credential_type
                || credential.schema_id != profile.schema_id
                || !profile.issuer_groups.contains(&credential.issuer_group)
                || profile.maximum_status_age_s.is_some_and(|maximum_age| {
                    now_s.saturating_sub(credential.status_checked_at_s) > maximum_age
                })
            {
                return Err(ProtocolProfileError::CredentialRequired);
            }
        }
        Ok(())
    }
}

fn parse_timestamp(value: &str) -> Result<i64, ProtocolProfileError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp())
        .map_err(|_| ProtocolProfileError::InvalidTimestamp)
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
        let policy = ProtocolProfile::from_json(include_bytes!(
            "../../../config/access/access-protocol-profile.json"
        ))
        .unwrap();
        let now_s = 1_787_900_100;
        let readiness = ReadinessEvidence {
            observed_at_ms: now_s * 1000,
            range_m: 1.12,
            closing_rate_mps: 0.18,
            checks: HashMap::new(),
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

        let mut stale = readiness;
        stale.observed_at_ms -= 501;
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
                &stale,
            ),
            Err(ProtocolProfileError::ReadinessStale)
        );
    }
}
