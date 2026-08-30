# Product Roadmap

This roadmap tracks the work required to mature the ACCESS authentication and
authorization core and its flight-software integration adapters. Simulation,
visualization, and scenario work is accepted only when it produces evidence for
the core or an adapter; it is not a separate product line.

## Phase 1: Executable reference model

**Status:** Complete

- deterministic Rust authorization reducer
- COSE/CBOR claims and signature verification
- replay and freshness rejection
- versioned protocol-profile and authorization-policy bundle schemas
- embedded ACCESS authorization policy over verified authority facts
- signed policy provenance in stage entitlements
- client authority Trust Bundle and entitlement verifier
- ROS 2/Gazebo docking reference environment
- deterministic allow and deny demonstrations

**Exit condition:** The reference environment clearly demonstrates that motion
cannot cross protected transitions without the required authorization evidence.

## Phase 2: Portable kernel and protocol

**Status:** In progress

- separate normative protocol types from reference-environment concepts
- publish a versioned CDDL wire specification
- support caller-provided clock and randomness
- define injectable, platform-qualified entropy (the Rust core now exposes a
  `RandomSource`; trusted clock values are already caller-provided to core APIs)
- define a production durable-state backend beyond the reference journal
- define revocation, degraded-clock, communication-loss, and recovery behavior
- provide a stable, reviewed C ABI
- evaluate a constrained or `no_std` Rust profile
- publish implementation-independent positive and negative test vectors

**Exit condition:** A non-ROS consumer can integrate and verify the kernel using
only the specification, C interface, and conformance vectors.

Required evidence:

- versioned CDDL and canonical encoding rules
- positive and negative vectors consumed by at least two implementations
- API/ABI compatibility tests and documented failure contracts
- deterministic tests using caller-provided time and entropy
- durability, corruption, rollback, and restart tests for the state backend

## Phase 3: Interoperability

- implement the production Rust ROS 2/Space ROS adapter
- add one flight-framework adapter, targeting cFS or F Prime
- define a DDS/IDL binding independent of ROS message packages
- demonstrate two independently implemented peers
- create protocol negotiation and compatibility tests
- document hardware key-store and secure-element integration points

**Exit condition:** Two different runtime stacks complete the same authorized
interaction and reject the same invalid vectors.

Adapter priority is a stable C ABI followed by cFS or F Prime. ROS 2 remains a
reference integration and validation harness unless separately qualified by an
integrator.

## Phase 4: Assurance

- maintain the published threat model and misuse-case analysis
- add fuzzing and property-based testing
- model-check the authorization and failure state machines
- produce reproducible and signed releases with software bills of materials
- establish vulnerability disclosure and security response processes
- map available evidence to relevant CCSDS, ECSS, NIST, and flight-assurance
  practices without claiming certification

**Exit condition:** Security and safety claims are traceable to public,
repeatable evidence rather than reference demonstrations alone.

The release gate also requires requirements-to-test traceability, MC/DC or the
project's selected critical-software coverage rationale, static analysis,
dependency and license review, reproducible toolchain records, fault-injection
results, and independent review of cryptographic and state-machine boundaries.

## Phase 5: Operational pilots

- work with design partners on real visiting-vehicle or servicing requirements
- integrate hardware-in-the-loop navigation and actuation boundaries
- validate mission-specific policy and safe-state behavior
- measure integration effort against existing custom approaches
- establish open protocol governance and a conformance program

**Exit condition:** At least one external organization uses the open interfaces
in a representative operational or qualification environment.

Operational acceptance is mission-specific. ACCESS evidence supports, but does
not replace, the integrator's hazard analysis, software assurance level,
qualification process, or launch authorization.
