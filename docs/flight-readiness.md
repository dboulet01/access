# ACCESS Production and Flight-Readiness Plan

## Purpose

This plan controls maturation of the ACCESS authentication and authorization
product and its integration adapters into production-ready software. It does
not declare the current repository flight ready or certified. A mission
integrator remains responsible for system
safety, platform qualification, secure communications, GNC, key provisioning,
and mission authorization.

## Controlled product boundary

### Product

1. **Portable core**: protocol and credential verification, trust evaluation,
   authorization policy, session binding, signed entitlements, replay defense,
   and protected-state reduction.
2. **Integration surface**: stable APIs and bindings for flight frameworks,
   mission transport, trusted time, qualified entropy, durable state, key
   stores, evidence providers, audit sinks, and enforcement points.
3. **Reference adapters**: implementations that demonstrate how to preserve the
   core's fail-closed contracts when connecting to a platform.

### Executable test suites

ROS 2 nodes and messages, Gazebo, the deterministic docking controller, web
components, Docker, Compose, fixture keys, and scenario policies are test
suites and fixtures. Their sole purpose is to demonstrate live behavior,
exercise integration boundaries, and produce evidence. They are not production
ACCESS components and are not part of a flight-software claim.

### External responsibilities

ACCESS does not implement or qualify:

- secure links, network admission, or mission PKI provisioning
- GNC, collision avoidance, sensors, actuators, or mechanical capture
- trusted platform time, hardware entropy, secure boot, or hardware roots of trust
- mission hazard analysis, operational policy approval, or certification

## Current readiness baseline

| Capability | State | Current evidence | Blocking gap |
| --- | --- | --- | --- |
| Fail-closed state transitions | Reference implementation | Rust unit tests and Docker smoke test | Formal requirements and independent analysis |
| COSE/Ed25519 verification | Reference implementation | Positive, tamper, issuer, audience, freshness, and replay tests | Published canonical vectors and cryptographic review |
| Authorization policy | Reference implementation | Cedar allow/deny tests and policy provenance | Policy lifecycle, rollback protection, and independent policy tooling |
| Entitlement consumption | Production interface with reference backend | Single-use verification, restart persistence, corrupt-state rejection, and backend commit-failure tests | Rollback-resistant production backend |
| Time | Core accepts caller time | Deterministic unit tests | Trusted-clock adapter contract and degraded-clock behavior |
| Entropy | Injectable core boundary | OS default and entropy-failure test | Qualified platform adapters and health/failure requirements |
| Signing | Process boundary demonstrated | Fixture signer and signature tests | HSM/secure-element adapter and key lifecycle |
| Flight framework integration | Not implemented | ROS 2 reference adapter only | Stable C ABI and cFS or F Prime adapter |
| Interoperability | Not demonstrated | One Rust implementation | CDDL, vectors, negotiation, second implementation |
| Assurance | Initial automated tests | Unit and end-to-end tests | Traceability, static analysis, fuzzing, coverage, reproducible releases |

## Workstreams and evidence gates

### FR-1: Normative protocol and conformance

Deliver versioned CDDL, canonical CBOR rules, algorithm identifiers, protocol
negotiation, error semantics, and positive/negative vectors. Every wire change
must identify compatibility impact.

**Gate:** two independent implementations accept every positive vector and
reject every negative vector with equivalent reason classes.

### FR-2: Portable deterministic kernel

Separate use-case fixtures from protocol types. Keep time caller-provided and
entropy injectable. Define bounded resource behavior, remove implicit process or
filesystem assumptions, evaluate `no_std`, and publish a reviewed stable C ABI.

**Gate:** a non-ROS harness integrates the library using only the published ABI,
specification, and vectors; deterministic replay produces identical decisions.

### FR-3: Platform security services

Define interfaces for monotonic trusted time, qualified entropy, signing,
configuration verification, audit export, and rollback-resistant durable state.
Implement at least one hardware-backed signer and one production state backend.

**Gate:** power-loss, corruption, rollback, clock-step, entropy-failure, key
rotation, and revoked-key tests fail closed with documented recovery behavior.

### FR-4: Flight-software adapters

Build adapters for cFS or F Prime after the C ABI stabilizes. Specify thread,
queue, deadline, memory, startup, shutdown, restart, transport, and authority
ownership contracts. Keep all GNC and actuation logic outside ACCESS.

**Gate:** the adapter passes framework-native tests, deadline and fault
injection, resource budgets, and an end-to-end protected-action demonstration.

### FR-5: Security and software assurance

Create requirements-to-code-to-test traceability; threat and misuse-case
analysis; static analysis; fuzz and property tests; state-machine model checking;
coverage rationale; dependency/license review; secure development and response
processes; and reproducible signed releases with SBOM and provenance.

The initial control set is maintained in
[assurance-requirements.md](assurance-requirements.md), and threats and residual
risk are maintained in [threat-model.md](threat-model.md).

**Gate:** an independent review closes all release-blocking findings and every
security claim points to repeatable evidence.

### FR-6: Mission integration and qualification

Integrate real identity, trust, policy, evidence, audit, and enforcement
providers in representative hardware. Define degraded and recovery behavior
with mission safety owners.

**Gate:** the mission's assurance authority accepts the integration evidence.
This gate cannot be satisfied by the reference simulation alone.

## Release policy

A release may be called a **reference release** while any FR-1 through FR-5 gate
is open. The term **flight-ready** requires all applicable gates, a named target
platform and assurance level, immutable toolchain and dependency records, and
written approval from the responsible mission assurance authority.

No simulation result, test count, or code-coverage percentage by itself permits
a flight-ready or certified claim.

## Immediate execution order

1. Keep CI green for formatting, linting, unit tests, configuration schemas, and
   Compose rendering.
2. Complete CDDL and checked-in conformance vectors.
3. Define the stable C ABI and explicit adapter failure contracts.
4. Implement and qualify a rollback-resistant backend behind the
   `ReplayStateBackend` interface.
5. Define clock-degradation and revocation state machines.
6. Add fuzzing, property tests, model checking, coverage, and traceability.
7. Implement and test the first cFS or F Prime adapter.
8. Integrate a hardware-backed signer and representative flight hardware.
