# ACCESS Project Principles

## Mission

ACCESS (**Access Control and Credential Evaluation for Space Systems**)
develops an open, portable, fail-closed authorization layer for
safety-critical interactions between independently operated spacecraft,
stations, servicing vehicles, and robotic systems.

The authorization protocol and core are independent of the included ROS 2 and
Gazebo reference environment.

## Scope

The project category is **spacecraft interaction authorization**. Its outcome is
that independently operated systems can establish trust, grant narrowly scoped
authority, approve individual safety-critical actions, and fail safely without
continuous ground control.

Docking is the first reference use case. The authorization model also applies
to servicing, refueling, capture, assembly, and shared resources.

## Guiding principles

### 1. Open interoperability

The protocol, core implementation, test vectors, schemas, and baseline adapters
are openly specified. Independent implementations do not require proprietary
tooling or the reference environment.

### 2. Simulator and platform independence

Authorization semantics must not depend on ROS 2, DDS, Gazebo, Python, a
particular operating system, or a particular dynamics model. These technologies
are adapters and reference infrastructure around a portable kernel.

### 3. Small trusted core

The security-critical kernel is deterministic, auditable, and isolated from
networking, filesystems, global state, and simulation concerns. Clocks,
randomness, cryptography, and durable storage enter through explicit interfaces.

### 4. Fail closed at the actuation boundary

Telemetry is not authority. Protected state transitions and actuator commands
must require a valid, unexpired, correctly scoped authorization. Missing,
ambiguous, stale, replayed, or unverifiable evidence results in `HOLD`, denial,
or a mission-defined safe abort.

### 5. Portable integration contracts

The kernel exposes stable contracts suitable for Rust and C. Adapters connect
ROS 2 and Space ROS, DDS, NASA cFS, F Prime, Basilisk, hardware-in-the-loop
systems, and embedded or real-time platforms.
Adapters translate transport and platform concepts; they do not redefine policy.

### 6. Evidence over claims

Security and interoperability claims are backed by executable test
vectors, negative tests, conformance suites, reproducible builds, threat models,
and versioned protocol documentation. Flight readiness must never be implied
without the corresponding verification, qualification, and mission evidence.

### 7. Safety and security evolve independently from dynamics

Simulation fidelity may progress from deterministic kinematics to high-fidelity
dynamics, contact mechanics, and flight hardware without changing authorization
semantics. Likewise, protocol and policy work must be testable without running a
simulator.

### 8. Explicit versioning and compatibility

Wire formats, policy schemas, state transitions, and ABI surfaces must be
versioned. Changes should remain backward compatible where safety permits and
fail clearly where compatibility cannot be guaranteed.
