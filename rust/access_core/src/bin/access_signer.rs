use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use ed25519_dalek::{Signer, SigningKey};
use serde::Deserialize;

type DynError = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct IdentityRecord {
    seed_hex: String,
}

#[derive(Deserialize)]
struct IdentityFile {
    fixture_only: bool,
    identities: HashMap<String, IdentityRecord>,
}

fn main() -> Result<(), DynError> {
    let (role, identities_path) = arguments()?;
    let file: IdentityFile = serde_json::from_slice(&fs::read(identities_path)?)?;
    if !file.fixture_only {
        return Err("file seed signer requires fixture_only=true".into());
    }
    let identity = file
        .identities
        .get(&role)
        .ok_or_else(|| format!("missing signing identity role: {role}"))?;
    let seed: [u8; 32] = hex::decode(&identity.seed_hex)?
        .try_into()
        .map_err(|_| "Ed25519 seed must contain exactly 32 bytes")?;

    let mut encoded_payload = String::new();
    io::stdin().read_to_string(&mut encoded_payload)?;
    let payload = hex::decode(encoded_payload.trim())?;
    let signature = SigningKey::from_bytes(&seed).sign(&payload);
    println!("{}", hex::encode(signature.to_bytes()));
    Ok(())
}

fn arguments() -> Result<(String, PathBuf), DynError> {
    let mut role = None;
    let mut identities_path = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--role" => role = arguments.next(),
            "--identities-file" => identities_path = arguments.next().map(PathBuf::from),
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    Ok((
        role.ok_or("--role is required")?,
        identities_path.ok_or("--identities-file is required")?,
    ))
}
