# ACCESS Test Strategy

## Scope

Tests provide evidence for the ACCESS core and integration contracts. ROS 2,
Gazebo, Docker, the controller, gateway, and dashboard are executable test
suites; passing them does not qualify those components as flight software.

## Test layers

| Layer | Purpose | Current execution |
| --- | --- | --- |
| Rust unit tests | Protocol, signatures, credentials, Cedar policy, profiles, replay, sessions, and protected-state reduction | `cargo test --workspace --all-targets` |
| Python adapter tests | Authority subprocess deadlines and simulation gateway allocation | `python -m pytest` from the repository root |
| Configuration conformance | Validate every JSON Schema and the checked-in profile, policy manifest, and client trust bundle | `python scripts/validate_config.py` |
| Compose validation | Detect invalid service/environment wiring | `docker compose config --quiet` |
| End-to-end reference test | Exercise ROS/Gazebo, authority, policy, signer, client verifier, and four protected transitions | `scripts/smoke_test.ps1` |

## Required CI gate

Every change must pass:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace --all-targets`
4. Python adapter tests
5. JSON Schema and checked-in configuration validation
6. Docker Compose rendering

The Dockerized end-to-end test should run on release candidates and on changes
to core protocol behavior, adapters, configuration, Docker, ROS messages, or
launch files.

## Evidence still required for flight readiness

- requirements-to-test traceability with stable requirement identifiers
- canonical positive and negative cross-implementation protocol vectors
- fuzzing of CBOR/COSE, JSON input, policy, and journal parsers
- property tests for replay, transition, and fail-closed invariants
- model checking of session, degraded-clock, communication-loss, revocation,
  restart, and protected-transition state machines
- structural coverage appropriate to the selected assurance level, with reviewed
  justification for uncovered defensive code
- worst-case execution time, memory, queue-depth, and storage-growth evidence on
  each target platform
- fault injection for signer, clock, entropy, persistence, transport, policy,
  evidence, and enforcement failures
- power-loss, torn-write, corruption, rollback, and recovery tests for the
  production state backend
- key rotation, compromise, revocation, and configuration rollback scenarios
- independent cryptographic, security, safety-interface, and source review
- reproducible release, dependency inventory, SBOM, provenance, and signature

## Test-data rules

Fixture private keys and deterministic entropy sources are permitted only in
tests and test fixtures and must be visibly marked. Production keys,
tokens, mission identifiers, and operational policy must never be committed.

Tests that accept caller-provided time or entropy must identify those providers
as deterministic test fixtures. Flight adapters must instead demonstrate their
trusted-time and qualified-entropy guarantees.

## Failure policy

Security-critical tests fail closed. A timeout, malformed input, missing trust,
unknown version, stale evidence, replay, persistence failure, entropy failure,
or verifier failure must never be converted to an allow result. Skipped tests
cannot satisfy a release gate without an approved, recorded rationale.
