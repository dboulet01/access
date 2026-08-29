use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use access_core::{
    AccessEngine, AccessEngineConfig, AccessScenario, CedarPolicyEngine, IdentityKey,
    PayloadSigner, ProtocolProfile, ReadinessEvidence, TransitionOutcome, TrustStore,
};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

const DEFAULT_IDENTITIES_PATH: &str = "config/access/simulation-identities.json";
const DEFAULT_TRUST_PATH: &str = "config/access/simulation-trust-bundle.json";
const DEFAULT_POLICY_PATH: &str = "config/access/access-protocol-profile.json";
const DEFAULT_AUTHORIZATION_POLICY_BUNDLE_PATH: &str =
    "config/access/access-authorization-policy-bundle.json";
const DEFAULT_STATE_PATH: &str = "state/access-authority";

type DynError = Box<dyn std::error::Error>;

struct ExternalSigner {
    command: PathBuf,
    role: String,
    identities_path: PathBuf,
}

impl PayloadSigner for ExternalSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, String> {
        let mut child = ProcessCommand::new(&self.command)
            .args(["--role", &self.role, "--identities-file"])
            .arg(&self.identities_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start {} signer: {error}", self.role))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "signer stdin unavailable".to_owned())?
            .write_all(hex::encode(payload).as_bytes())
            .map_err(|error| format!("could not write signer request: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("signer process failed: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        hex::decode(String::from_utf8_lossy(&output.stdout).trim())
            .map_err(|error| format!("signer returned invalid signature encoding: {error}"))
    }
}

#[derive(Deserialize)]
struct TrustBundle {
    bundle_id: String,
    version: u64,
    issued_at: String,
    keys: Vec<TrustRecord>,
}

#[derive(Deserialize)]
struct TrustRecord {
    key_id: String,
    public_key_hex: String,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct AuthorizationPolicyBundle {
    bundle_id: String,
    bundle_version: u64,
    issued_at: String,
    valid_until: String,
    source_file: PathBuf,
}

impl AuthorizationPolicyBundle {
    fn load(path: &Path, now_s: i64) -> Result<(Self, String), DynError> {
        let bundle: Self = serde_json::from_slice(&fs::read(path)?)?;
        let issued_at = chrono::DateTime::parse_from_rfc3339(&bundle.issued_at)?.timestamp();
        let valid_until = chrono::DateTime::parse_from_rfc3339(&bundle.valid_until)?.timestamp();
        if now_s < issued_at || now_s > valid_until {
            return Err(
                "ACCESS authorization policy bundle is outside its validity interval".into(),
            );
        }
        let source = fs::read_to_string(&bundle.source_file)?;
        Ok((bundle, source))
    }
}

impl TrustBundle {
    fn load(path: &Path) -> Result<Self, DynError> {
        let bundle: Self = serde_json::from_slice(&fs::read(path)?)?;
        if bundle.bundle_id.trim().is_empty() {
            return Err("trust bundle_id must not be empty".into());
        }
        Ok(bundle)
    }

    fn store_for(&self, scope: &str) -> Result<TrustStore, DynError> {
        let mut store = TrustStore::default();
        let mut count = 0;
        for record in self
            .keys
            .iter()
            .filter(|key| key.scopes.iter().any(|value| value == scope))
        {
            let bytes: [u8; 32] = hex::decode(&record.public_key_hex)?
                .try_into()
                .map_err(|_| "Ed25519 public key must contain exactly 32 bytes")?;
            store.insert(&record.key_id, VerifyingKey::from_bytes(&bytes)?);
            count += 1;
        }
        if count == 0 {
            return Err(format!("trust bundle has no keys for scope: {scope}").into());
        }
        Ok(store)
    }

    fn external_identity(
        &self,
        scope: &str,
        role: &str,
        command: &Path,
        identities_path: &Path,
    ) -> Result<IdentityKey, DynError> {
        let records: Vec<_> = self
            .keys
            .iter()
            .filter(|key| key.scopes.iter().any(|value| value == scope))
            .collect();
        let [record] = records.as_slice() else {
            return Err(format!("trust scope must contain exactly one key: {scope}").into());
        };
        let bytes: [u8; 32] = hex::decode(&record.public_key_hex)?
            .try_into()
            .map_err(|_| "Ed25519 public key must contain exactly 32 bytes")?;
        Ok(IdentityKey::from_signer(
            &record.key_id,
            VerifyingKey::from_bytes(&bytes)?,
            ExternalSigner {
                command: command.to_owned(),
                role: role.to_owned(),
                identities_path: identities_path.to_owned(),
            },
        ))
    }
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Command {
    Describe,
    Establish {
        #[serde(default)]
        scenario: String,
        now_s: Option<i64>,
    },
    Transition {
        requested_state: u8,
        now_s: Option<i64>,
        readiness: ReadinessEvidence,
    },
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
    let identity_path = env_path("ACCESS_IDENTITIES_FILE", DEFAULT_IDENTITIES_PATH);
    let signer_command = env::var_os("ACCESS_SIGNER_COMMAND")
        .map(PathBuf::from)
        .unwrap_or_else(default_signer_command);

    let trust_path = env_path("ACCESS_TRUST_BUNDLE_FILE", DEFAULT_TRUST_PATH);
    let trust = TrustBundle::load(&trust_path)?;
    let policy_path = env_path("ACCESS_PROTOCOL_PROFILE_FILE", DEFAULT_POLICY_PATH);
    let protocol_profile = ProtocolProfile::from_json(&fs::read(policy_path)?)?;
    let authorization_policy_bundle_path = env_path(
        "ACCESS_AUTHORIZATION_POLICY_BUNDLE_FILE",
        DEFAULT_AUTHORIZATION_POLICY_BUNDLE_PATH,
    );
    let (authorization_policy_bundle, authorization_policy_source) =
        AuthorizationPolicyBundle::load(&authorization_policy_bundle_path, now_s)?;
    let authorization_policy_engine = CedarPolicyEngine::from_source(
        authorization_policy_bundle.bundle_id,
        authorization_policy_bundle.bundle_version,
        &authorization_policy_source,
    )?;
    let trust_bundle_issued_at_s =
        chrono::DateTime::parse_from_rfc3339(&trust.issued_at)?.timestamp();
    let mut engine = AccessEngine::new(
        trust.external_identity("station_peer", "chaser", &signer_command, &identity_path)?,
        trust.external_identity("chaser_peer", "station", &signer_command, &identity_path)?,
        trust.external_identity(
            "credential_issuer",
            "credential_issuer",
            &signer_command,
            &identity_path,
        )?,
        trust.store_for("station_peer")?,
        trust.store_for("chaser_peer")?,
        trust.store_for("credential_issuer")?,
        trust.store_for("transition_gate")?,
        AccessEngineConfig {
            protocol_profile,
            authorization_policy_engine,
            trust_bundle_id: trust.bundle_id.clone(),
            trust_bundle_version: trust.version,
            trust_bundle_issued_at_s,
        },
    );
    engine.enable_persistent_state(env_path("ACCESS_STATE_DIR", DEFAULT_STATE_PATH))?;

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) => handle_command(&mut engine, &line),
            Err(error) => error_response(error.to_string()),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

fn handle_command(engine: &mut AccessEngine, input: &str) -> Response<serde_json::Value> {
    let result = (|| -> Result<serde_json::Value, DynError> {
        let command: Command = serde_json::from_str(input)?;
        match command {
            Command::Describe => {
                let mut description = serde_json::to_value(engine.protocol_profile())?;
                description
                    .as_object_mut()
                    .ok_or("protocol profile description must be an object")?
                    .insert(
                        "authorization_policy".into(),
                        serde_json::to_value(engine.authorization_policy())?,
                    );
                Ok(description)
            }
            Command::Establish { scenario, now_s } => {
                let scenario = parse_scenario(&scenario)?;
                Ok(serde_json::to_value(engine.establish_session(
                    now_s.unwrap_or_else(unix_time),
                    scenario,
                )?)?)
            }
            Command::Transition {
                requested_state,
                now_s,
                readiness,
            } => {
                let outcome: TransitionOutcome = engine.request_transition(
                    requested_state,
                    now_s.unwrap_or_else(unix_time),
                    &readiness,
                )?;
                Ok(serde_json::to_value(outcome)?)
            }
        }
    })();
    match result {
        Ok(value) => Response {
            ok: true,
            value: Some(value),
            error: None,
        },
        Err(error) => error_response(error.to_string()),
    }
}

fn error_response(error: String) -> Response<serde_json::Value> {
    Response {
        ok: false,
        value: None,
        error: Some(error),
    }
}

fn parse_scenario(value: &str) -> Result<AccessScenario, DynError> {
    match value {
        "" | "nominal" => Ok(AccessScenario::Nominal),
        "expired_credential" => Ok(AccessScenario::ExpiredCredential),
        "corridor_violation" => Ok(AccessScenario::CorridorViolation),
        "latch_not_ready" => Ok(AccessScenario::LatchNotReady),
        _ => Err(format!("unknown scenario: {value}").into()),
    }
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

fn default_signer_command() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned))
        .map(|path| path.join(format!("access-signer{}", env::consts::EXE_SUFFIX)))
        .unwrap_or_else(|| PathBuf::from("access-signer"))
}
