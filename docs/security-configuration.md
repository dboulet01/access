# Security Configuration

## Reference configuration

The simulation loads signing identities and public trust independently:

- `config/access/simulation-identities.json` contains deliberately public,
  deterministic Ed25519 private seeds for repeatable simulation only.
- `config/access/simulation-trust-bundle.json` contains public keys with separate
  station-peer, chaser-peer, credential-issuer, and transition-gate scopes.
- `config/access/access-authorization-policy-bundle.json` selects the active
  ACCESS authorization policy source, monotonic bundle version, and validity window.
- `config/access/simulation-client-trust-bundle.json` contains the authority IDs
  and entitlement-signing keys accepted by the Odyssey-7 client.
- `.env.example` documents the runtime selectors. Real `.env` and `keys/`
  content are ignored by Git. `compose.yaml` explicitly maps these selectors
  into each ACCESS simulation container; Compose's `.env` parsing alone does
  not inject variables into a container.

The file provider refuses identity documents that do not declare
`fixture_only: true`. This prevents it from being presented as a production key
loader; it does not make checked-in seeds safe.

| Variable | Purpose |
| --- | --- |
| `ACCESS_AUTHORITY_COMMAND` | Authority process started by the ROS adapter |
| `ACCESS_AUTHORITY_TIMEOUT_S` | Fail-closed response deadline for authority IPC |
| `ACCESS_SIGNER_COMMAND` | Role-bound signing process; replace with an HSM/KMS adapter |
| `ACCESS_IDENTITIES_FILE` | Simulation signer identity fixture |
| `ACCESS_TRUST_BUNDLE_FILE` | Authority-side peer, issuer, and gate verification keys |
| `ACCESS_PROTOCOL_PROFILE_FILE` | Rust protocol, freshness, state, and entitlement profile |
| `ACCESS_AUTHORIZATION_POLICY_BUNDLE_FILE` | Active ACCESS authorization policy bundle manifest |
| `ACCESS_STATE_DIR` | Durable nonce and consumed-entitlement journals |
| `ACCESS_ENTITLEMENT_VERIFIER_COMMAND` | Client verifier process started by the ROS adapter |
| `ACCESS_ENTITLEMENT_VERIFIER_TIMEOUT_S` | Fail-closed client verification deadline |
| `ACCESS_CLIENT_TRUST_BUNDLE_FILE` | Client's trusted authority IDs and public keys |
| `ACCESS_CLIENT_REPLAY_FILE` | Client's durable accepted-entitlement journal |

## Trust and policy domains

Three configuration domains are intentionally separate:

| Domain | Question answered | Consumer |
| --- | --- | --- |
| Authority Trust Bundle | Which peers, credential issuers, and key purposes may establish verified facts? | AAS authentication core |
| ACCESS authorization policy bundle | Which actions may an authenticated principal perform under current verified conditions? | APS |
| Client authority Trust Bundle | Which authorities and keys may issue entitlements to this client? | AC verifier |

The client verifier rejects an approved response unless the entitlement is an
Ed25519 COSE entitlement signed by the expected trusted authority and
bound to the client recipient, active session, requested stage, and validity
window. It durably rejects replay. This is similar to configured issuer/key
validation by an OIDC relying party, but it is local ACCESS configuration and
does not require live issuer discovery.

The domains are not interchangeable. Authorization policy evaluation cannot establish cryptographic facts,
and channel authentication cannot substitute for entitlement verification. The
reference files are deterministic development fixtures, not authenticated
distribution artifacts.

## Production replacement

The stable reference integration boundary is the authority process's newline-
delimited JSON request/response contract. A replacement may change the internal
AAS/APS/AEG deployment while preserving peer protocol and fail-closed behavior.

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
- provision client authority Trust Bundles independently from station issuer
  Trust Bundles and retain overlap during controlled key rotation

The reference authority loads only public verification keys. Each signature is
delegated to a role-bound `access-signer` process; only that process opens the
simulation seed fixture. Consumed nonces and entitlement IDs are appended and synced
under `ACCESS_STATE_DIR`, with independent persistent volumes per simulator.
The client verifier likewise loads only public authority keys and appends
accepted entitlement nonces to `ACCESS_CLIENT_REPLAY_FILE` before returning
success.
Production deployments must replace the fixture signer and harden journal
integrity, access control, retention, and rollback protection.

## Platform service contracts

The portable Rust API receives time values from its caller for session and
transition processing. Reference command-line processes use system time when a
request does not supply `now_s`. A flight adapter must source time from the
mission's trusted clock, detect loss of validity and unacceptable steps, and
withhold authorization while time quality is insufficient. A caller-provided
timestamp is not trusted merely because it is present.

`AccessEngine` uses `OsRandomSource` by default and permits a platform adapter
through the `RandomSource` trait. A flight implementation must use a qualified
cryptographic entropy source, monitor its health as required by the platform,
and treat entropy failure as denial. Deterministic implementations are allowed
only in conformance tests.

`PayloadSigner` is the key-operation boundary. A production adapter must keep
private keys inside an approved HSM, secure element, or platform key service;
enforce key purpose and role; expose no private material; and define timeout,
availability, rotation, revocation, and zeroization behavior.

`ReplayCache` accepts a `ReplayStateBackend`. The included file backend
synchronizes every consumed identifier and fails closed on malformed records;
it exists for executable test suites and reference deployments. A production
backend must additionally provide atomicity appropriate to its storage,
rollback detection, integrity and access control, bounded retention, health
reporting, and tested recovery after power loss or interrupted writes.

These contracts are integration requirements, not capabilities supplied by
Docker, ROS 2, or the simulation fixtures. Their acceptance evidence is tracked
in [flight-readiness.md](flight-readiness.md).