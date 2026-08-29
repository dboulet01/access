use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use access_core::{MessageType, ReplayCache, TrustStore, verify_envelope};
use chrono::DateTime;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

const DEFAULT_CLIENT_TRUST_PATH: &str = "config/access/simulation-client-trust-bundle.json";
const DEFAULT_REPLAY_PATH: &str = "state/access-client/consumed-entitlements.log";
const MAX_CLOCK_SKEW_S: i64 = 30;

type DynError = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct ClientTrustBundle {
    bundle_id: String,
    version: u64,
    client_id: String,
    valid_from: String,
    valid_until: String,
    trusted_authorities: Vec<TrustedAuthority>,
}

#[derive(Deserialize)]
struct TrustedAuthority {
    authority_id: String,
    signing_keys: Vec<SigningKeyRecord>,
}

#[derive(Deserialize)]
struct SigningKeyRecord {
    key_id: String,
    algorithm: String,
    public_key_hex: String,
}

impl ClientTrustBundle {
    fn load(path: &Path, now_s: i64) -> Result<Self, DynError> {
        let bundle: Self = serde_json::from_slice(&fs::read(path)?)?;
        let valid_from = DateTime::parse_from_rfc3339(&bundle.valid_from)?.timestamp();
        let valid_until = DateTime::parse_from_rfc3339(&bundle.valid_until)?.timestamp();
        if now_s < valid_from || now_s > valid_until {
            return Err("client trust bundle is outside its validity interval".into());
        }
        if bundle.bundle_id.trim().is_empty() || bundle.client_id.trim().is_empty() {
            return Err("client trust bundle identifiers must not be empty".into());
        }
        Ok(bundle)
    }

    fn trust_store(&self) -> Result<TrustStore, DynError> {
        let mut store = TrustStore::default();
        let mut count = 0;
        for authority in &self.trusted_authorities {
            for key in &authority.signing_keys {
                if key.algorithm != "Ed25519" {
                    return Err(
                        format!("unsupported entitlement algorithm: {}", key.algorithm).into(),
                    );
                }
                let bytes: [u8; 32] = hex::decode(&key.public_key_hex)?
                    .try_into()
                    .map_err(|_| "Ed25519 public key must contain exactly 32 bytes")?;
                store.insert(&key.key_id, VerifyingKey::from_bytes(&bytes)?);
                count += 1;
            }
        }
        if count == 0 {
            return Err("client trust bundle contains no authority signing keys".into());
        }
        Ok(store)
    }

    fn trusts_authority(&self, authority_id: &str, key_id: &str) -> bool {
        self.trusted_authorities.iter().any(|authority| {
            authority.authority_id == authority_id
                && authority
                    .signing_keys
                    .iter()
                    .any(|key| key.key_id == key_id)
        })
    }
}

#[derive(Deserialize)]
struct VerifyCommand {
    command: String,
    entitlement_hex: Option<String>,
    expected_authority: Option<String>,
    expected_session_id: Option<String>,
    expected_stage: Option<String>,
    now_s: Option<i64>,
}

#[derive(Serialize)]
struct VerifiedEntitlement {
    verified: bool,
    trust_bundle_id: String,
    trust_bundle_version: u64,
    authority_id: String,
    client_id: String,
    grant_id: String,
    session_id: String,
    authorized_stage: String,
    expires_at_s: i64,
    protocol_profile_id: String,
    protocol_profile_version: u64,
    rule_id: String,
    authorization_policy_bundle_id: String,
    authorization_policy_bundle_version: u64,
    authorization_policy_sha256: String,
}

#[derive(Serialize)]
struct Response<T: Serialize> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() -> Result<(), DynError> {
    let now_s = unix_time();
    let path = env_path("ACCESS_CLIENT_TRUST_BUNDLE_FILE", DEFAULT_CLIENT_TRUST_PATH);
    let bundle = ClientTrustBundle::load(&path, now_s)?;
    let trust = bundle.trust_store()?;
    let replay_path = env_path("ACCESS_CLIENT_REPLAY_FILE", DEFAULT_REPLAY_PATH);
    let mut replay = ReplayCache::persistent(replay_path)?;

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) => match verify_command(&line, &bundle, &trust, &mut replay) {
                Ok(value) => Response {
                    ok: true,
                    value: Some(value),
                    error: None,
                },
                Err(error) => Response {
                    ok: false,
                    value: None,
                    error: Some(error.to_string()),
                },
            },
            Err(error) => Response {
                ok: false,
                value: None,
                error: Some(error.to_string()),
            },
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

fn verify_command(
    input: &str,
    bundle: &ClientTrustBundle,
    trust: &TrustStore,
    replay: &mut ReplayCache,
) -> Result<VerifiedEntitlement, DynError> {
    let command: VerifyCommand = serde_json::from_str(input)?;
    if command.command == "describe" {
        return Err("describe is not a verification command".into());
    }
    if command.command != "verify_entitlement" {
        return Err(format!("unknown command: {}", command.command).into());
    }
    let encoded = hex::decode(
        command
            .entitlement_hex
            .as_deref()
            .ok_or("entitlement_hex is required")?,
    )?;
    let now_s = command.now_s.unwrap_or_else(unix_time);
    let verified = verify_envelope(
        &encoded,
        trust,
        &bundle.client_id,
        replay,
        now_s,
        MAX_CLOCK_SKEW_S,
    )?;
    if verified.claims.message_type != MessageType::AuthorizationGrant {
        return Err("message is not an ACCESS authorization grant".into());
    }
    let expected_authority = command
        .expected_authority
        .as_deref()
        .ok_or("expected_authority is required")?;
    if verified.signer != expected_authority
        || !bundle.trusts_authority(expected_authority, &verified.signer)
    {
        return Err("entitlement issuer is not the expected trusted authority".into());
    }
    let expires_at_s = verified.claims.expires_at_s.ok_or("grant has no expiry")?;
    if now_s > expires_at_s {
        return Err("grant is expired".into());
    }
    let session_id = verified
        .claims
        .session_id
        .ok_or("grant has no session binding")?;
    if command.expected_session_id.as_deref() != Some(session_id.as_str()) {
        return Err("grant session does not match the active ACCESS session".into());
    }
    let authorized_stage = serde_json::to_value(
        verified
            .claims
            .authorized_stage
            .ok_or("grant has no authorized stage")?,
    )?
    .as_str()
    .ok_or("authorized stage is invalid")?
    .to_owned();
    if command.expected_stage.as_deref() != Some(authorized_stage.as_str()) {
        return Err("grant stage does not match the requested operation".into());
    }

    Ok(VerifiedEntitlement {
        verified: true,
        trust_bundle_id: bundle.bundle_id.clone(),
        trust_bundle_version: bundle.version,
        authority_id: verified.signer,
        client_id: bundle.client_id.clone(),
        grant_id: verified.claims.grant_id.ok_or("grant has no identifier")?,
        session_id,
        authorized_stage,
        expires_at_s,
        protocol_profile_id: verified
            .claims
            .protocol_profile_id
            .ok_or("grant has no protocol profile")?,
        protocol_profile_version: verified
            .claims
            .protocol_profile_version
            .ok_or("grant has no protocol profile version")?,
        rule_id: verified.claims.rule_id.ok_or("grant has no rule binding")?,
        authorization_policy_bundle_id: verified
            .claims
            .authorization_policy_bundle_id
            .ok_or("grant has no ACCESS authorization policy bundle binding")?,
        authorization_policy_bundle_version: verified
            .claims
            .authorization_policy_bundle_version
            .ok_or("grant has no ACCESS authorization policy bundle version")?,
        authorization_policy_sha256: verified
            .claims
            .authorization_policy_sha256
            .ok_or("grant has no ACCESS authorization policy digest")?,
    })
}

fn env_path(name: &str, default: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use access_core::{AuthorizedStage, IdentityKey, ProtocolClaims, sign_envelope};

    use super::*;

    const NOW_S: i64 = 1_787_900_100;

    fn fixture() -> (ClientTrustBundle, TrustStore, String) {
        let station = IdentityKey::from_seed("waystation-1", [2; 32]);
        let bundle = ClientTrustBundle {
            bundle_id: "client-trust".into(),
            version: 1,
            client_id: "odyssey-7".into(),
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_until: "2027-01-01T00:00:00Z".into(),
            trusted_authorities: vec![TrustedAuthority {
                authority_id: "waystation-1".into(),
                signing_keys: vec![SigningKeyRecord {
                    key_id: "waystation-1".into(),
                    algorithm: "Ed25519".into(),
                    public_key_hex: hex::encode(station.verifying_key().as_bytes()),
                }],
            }],
        };
        let encoded = sign_envelope(
            &ProtocolClaims {
                message_type: MessageType::AuthorizationGrant,
                issuer: "waystation-1".into(),
                recipient: "odyssey-7".into(),
                issued_at_s: NOW_S,
                nonce: vec![7; 32],
                session_id: Some("session-1".into()),
                authorized_stage: Some(AuthorizedStage::Approach),
                challenge_nonce: None,
                credentials: vec![],
                grant_id: Some("grant-1".into()),
                expires_at_s: Some(NOW_S + 30),
                protocol_profile_id: Some("protocol-profile".into()),
                protocol_profile_version: Some(3),
                rule_id: Some("enter-approach".into()),
                authorization_policy_bundle_id: Some("commercial-policy".into()),
                authorization_policy_bundle_version: Some(2),
                authorization_policy_sha256: Some("sha-256:abc".into()),
            },
            &station,
        )
        .unwrap();
        let trust = bundle.trust_store().unwrap();
        (bundle, trust, hex::encode(encoded))
    }

    #[test]
    fn verifies_expected_authority_and_rejects_replay() {
        let (bundle, trust, entitlement_hex) = fixture();
        let input = serde_json::json!({
            "command": "verify_entitlement",
            "entitlement_hex": entitlement_hex,
            "expected_authority": "waystation-1",
            "expected_session_id": "session-1",
            "expected_stage": "approach",
            "now_s": NOW_S
        })
        .to_string();
        let mut replay = ReplayCache::default();
        let verified = verify_command(&input, &bundle, &trust, &mut replay).unwrap();
        assert_eq!(verified.grant_id, "grant-1");
        assert!(verify_command(&input, &bundle, &trust, &mut replay).is_err());
    }

    #[test]
    fn rejects_wrong_authority() {
        let (bundle, trust, entitlement_hex) = fixture();
        let input = serde_json::json!({
            "command": "verify_entitlement",
            "entitlement_hex": entitlement_hex,
            "expected_authority": "rogue-station",
            "expected_session_id": "session-1",
            "expected_stage": "approach",
            "now_s": NOW_S
        })
        .to_string();
        assert!(verify_command(&input, &bundle, &trust, &mut ReplayCache::default()).is_err());
    }
}
