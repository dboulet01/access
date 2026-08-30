# ACCESS Assurance Requirements

## Purpose

These identifiers begin requirements-to-evidence traceability for the production
ACCESS core and integration interfaces. Status means evidence exists in this
repository; it does not imply certification or mission acceptance.

## Core security requirements

| ID | Requirement | Current evidence | Status |
| --- | --- | --- | --- |
| ACC-SEC-001 | The core shall reject envelopes with an untrusted signer, invalid signature, unsupported algorithm, issuer mismatch, or recipient mismatch. | Protocol unit tests and client-verifier tests | Partial |
| ACC-SEC-002 | The core shall reject messages outside the configured freshness window. | Protocol and profile validation tests | Partial |
| ACC-SEC-003 | The core shall reject a previously consumed nonce or entitlement identifier. | Replay and verifier tests | Implemented |
| ACC-SEC-004 | Durable consumption shall complete before an allow result is committed. | `ReplayStateBackend` commit-failure test | Implemented at interface |
| ACC-SEC-005 | The core shall fail closed when entropy generation fails. | `RandomSource` failure test | Implemented |
| ACC-SEC-006 | Every grant shall bind authority, recipient, session, action, policy provenance, validity, and a unique identifier. | Session transition tests | Partial |
| ACC-SEC-007 | Mandatory readiness evidence shall be fresh and satisfied before policy permits a protected action. | Corridor and latch denial tests | Partial |
| ACC-SEC-008 | Missing authorization context shall never release a protected transition. | State-machine fail-closed tests | Implemented |
| ACC-SEC-009 | Policy and trust configuration shall be versioned and validity bounded. | Profile, policy-bundle, and trust-bundle parsing and schema validation | Partial |
| ACC-SEC-010 | Unknown protocol or policy versions shall be rejected, not interpreted as a compatible version. | Planned conformance vectors | Open |

## Platform integration requirements

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| ACC-INT-001 | The time adapter shall provide authenticated mission time and report degraded or invalid quality. | Clock-step, loss, recovery, and boundary tests on target platform | Open |
| ACC-INT-002 | The entropy adapter shall use a platform-qualified cryptographic source and report failure. | Adapter qualification and fault injection | Open |
| ACC-INT-003 | The signing adapter shall protect private keys and enforce key role and purpose. | HSM/secure-element integration tests and key lifecycle review | Open |
| ACC-INT-004 | The state backend shall detect corruption and rollback and provide atomic durable consumption. | Power-loss, snapshot rollback, corruption, and recovery campaign | Open |
| ACC-INT-005 | The evidence adapter shall authenticate source, freshness, quality, and provenance. | Framework permissions and evidence fault injection | Open |
| ACC-INT-006 | The enforcement adapter shall atomically consume one entitlement before releasing exactly one action. | Target integration and concurrency tests | Open |
| ACC-INT-007 | Every adapter shall define bounded queues, deadlines, memory, restart, and unavailable-service behavior. | WCET/resource analysis and failure tests | Open |
| ACC-INT-008 | All adapter errors and deadline misses shall preserve the closed enforcement state. | Fault-injection matrix | Open |

## Assurance and release requirements

| ID | Requirement | Required evidence | Status |
| --- | --- | --- | --- |
| ACC-ASR-001 | Normative wire behavior shall be specified independently of Rust and ROS. | Versioned CDDL and canonical encoding rules | Open |
| ACC-ASR-002 | At least two independent implementations shall pass common positive and negative vectors. | Interoperability report | Open |
| ACC-ASR-003 | The public integration ABI shall be versioned and compatibility tested. | Stable C ABI and ABI test suite | Open |
| ACC-ASR-004 | Security-critical parsers and state machines shall undergo fuzz, property, and model-based testing. | Reproducible reports and retained counterexamples | Open |
| ACC-ASR-005 | Requirements shall trace to design, source, tests, results, and reviewed anomalies. | Generated traceability matrix | In progress |
| ACC-ASR-006 | Releases shall be reproducible, signed, and accompanied by SBOM and provenance. | Independent rebuild and signature verification | Open |
| ACC-ASR-007 | Security and cryptographic boundaries shall receive independent review. | Closed review report | Open |
| ACC-ASR-008 | A release shall not claim flight readiness without a named platform, assurance target, and mission-authority acceptance. | Release approval record | Open |

## Maintenance

A change that affects authentication, authorization, entitlement semantics,
state consumption, trust, policy, time, entropy, signing, or enforcement must
update this matrix and its linked evidence. Test-suite behavior should map to a
requirement; simulation features without a core or adapter evidence purpose are
out of scope.
