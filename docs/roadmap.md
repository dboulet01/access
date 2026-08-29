# Product Roadmap

This roadmap tracks the work required for a portable, interoperable
authorization implementation.

## Phase 1: Executable reference model

**Status:** Complete

- deterministic Rust authorization reducer
- COSE/CBOR claims and signature verification
- replay and freshness rejection
- policy schemas and example decisions
- ROS 2/Gazebo docking reference environment
- deterministic allow and deny demonstrations

**Exit condition:** The reference environment clearly demonstrates that motion
cannot cross protected transitions without the required authorization evidence.

## Phase 2: Portable kernel and protocol

- separate normative protocol types from reference-environment concepts
- publish a versioned CDDL wire specification
- support caller-provided clock and randomness
- define a production durable-state backend beyond the reference journal
- define revocation, degraded-clock, communication-loss, and recovery behavior
- provide a stable, reviewed C ABI
- evaluate a constrained or `no_std` Rust profile
- publish implementation-independent positive and negative test vectors

**Exit condition:** A non-ROS consumer can integrate and verify the kernel using
only the specification, C interface, and conformance vectors.

## Phase 3: Interoperability

- implement the production Rust ROS 2/Space ROS adapter
- add one flight-framework adapter, targeting cFS or F Prime
- define a DDS/IDL binding independent of ROS message packages
- demonstrate two independently implemented peers
- create protocol negotiation and compatibility tests
- document hardware key-store and secure-element integration points

**Exit condition:** Two different runtime stacks complete the same authorized
interaction and reject the same invalid vectors.

## Phase 4: Assurance

- publish a threat model and misuse-case analysis
- add fuzzing and property-based testing
- model-check the authorization and failure state machines
- produce reproducible and signed releases with software bills of materials
- establish vulnerability disclosure and security response processes
- map available evidence to relevant CCSDS, ECSS, NIST, and flight-assurance
  practices without claiming certification

**Exit condition:** Security and safety claims are traceable to public,
repeatable evidence rather than reference demonstrations alone.

## Phase 5: Operational pilots

- work with design partners on real visiting-vehicle or servicing requirements
- integrate hardware-in-the-loop navigation and actuation boundaries
- validate mission-specific policy and safe-state behavior
- measure integration effort against existing custom approaches
- establish open protocol governance and a conformance program

**Exit condition:** At least one external organization uses the open interfaces
in a representative operational or qualification environment.
