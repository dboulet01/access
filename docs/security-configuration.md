# Security Configuration

## Reference configuration

The simulation loads signing identities and public trust independently:

- `config/access/simulation-identities.json` contains deliberately public,
  deterministic Ed25519 private seeds for repeatable simulation only.
- `config/access/simulation-trust-bundle.json` contains public keys with separate
  station-peer, chaser-peer, credential-issuer, and transition-gate scopes.
- `.env.example` documents the runtime selectors. Real `.env` and `keys/`
  content are ignored by Git.

The file provider refuses identity documents that do not declare
`fixture_only: true`. This prevents it from being presented as a production key
loader; it does not make checked-in seeds safe.

| Variable | Purpose |
| --- | --- |
| `ACCESS_AUTHORITY_COMMAND` | Authority process started by the ROS adapter |
| `ACCESS_AUTHORITY_TIMEOUT_S` | Fail-closed response deadline for authority IPC |
| `ACCESS_SIGNER_COMMAND` | Role-bound signing process; replace with an HSM/KMS adapter |
| `ACCESS_IDENTITIES_FILE` | Simulation signer identity fixture |
| `ACCESS_TRUST_BUNDLE_FILE` | Public, purpose-scoped verification keys |
| `ACCESS_POLICY_FILE` | Executable authorization policy |
| `ACCESS_STATE_DIR` | Durable nonce and consumed-grant journals |

## Production replacement

The stable integration boundary is the authority process's newline-delimited
JSON request/response contract. A production implementation can replace
`ACCESS_AUTHORITY_COMMAND` without changing ROS, Gazebo, or the controller. It
should preserve the fail-closed response contract while using an HSM, KMS, or
platform keystore for non-exportable signing keys.

A production deployment must also:

- authenticate and authorize configuration bundles before activation
- enforce monotonic bundle versions and rollback protection
- mount public trust and policy configuration read-only
- use durable, atomic replay and entitlement-consumption state
- obtain time from a trusted source and define clock-failure behavior
- separate issuer, station, vehicle, and transition-gate key purposes
- rotate and revoke keys without silently retaining stale trust
- protect the ROS decision publisher with SROS 2 governance and permissions
- retain signed audit records without private key material

The reference authority loads only public verification keys. Each signature is
delegated to a role-bound `access-signer` process; only that process opens the
simulation seed fixture. Consumed nonces and grant IDs are appended and synced
under `ACCESS_STATE_DIR`, with independent persistent volumes per simulator.
Production deployments must replace the fixture signer and harden journal
integrity, access control, retention, and rollback protection.