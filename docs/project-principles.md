# Project Principles

## Mission

This project develops an open, portable, fail-closed authorization layer for
safety-critical interactions between independently operated spacecraft,
stations, servicing vehicles, and robotic systems.

The product is the authorization protocol, security kernel, stable integration
contracts, and conformance evidence. The included simulation is a reference
environment used to build operational credibility by demonstrating, testing,
and explaining those capabilities. It is not the product boundary and must not
become a dependency of the security core.

## Outcome and category

The project category is **Spacecraft Interaction Authorization**. Its outcome is
that independently operated systems can establish trust, grant narrowly scoped
authority, approve individual safety-critical actions, and fail safely without
continuous ground control.

Project communication should lead with that operational outcome. Cryptography,
identity formats, ROS 2, DDS, Rust, and simulation explain how the system works;
they are not the primary value proposition. Docking is the first reference use
case, not the limit of the product.

See [positioning.md](positioning.md) for the canonical problem and product
description.

## Guiding principles

### 1. Open interoperability

The protocol, core implementation, test vectors, schemas, and baseline adapters
should be openly available under a commercially usable license. Independent
implementations must be possible without access to proprietary tooling or the
reference simulation.

### 2. Simulator and platform independence

Authorization semantics must not depend on ROS 2, DDS, Gazebo, Python, a
particular operating system, or a particular dynamics model. These technologies
are adapters and reference infrastructure around a portable kernel.

### 3. Small trusted core

The security-critical kernel should remain deterministic, auditable, and small.
It should avoid networking, filesystems, global state, and simulation concerns.
Platform capabilities such as clocks, randomness, cryptography, and durable
storage should enter through explicit interfaces.

### 4. Fail closed at the actuation boundary

Telemetry is not authority. Protected state transitions and actuator commands
must require a valid, unexpired, correctly scoped authorization. Missing,
ambiguous, stale, replayed, or unverifiable evidence results in `HOLD`, denial,
or a mission-defined safe abort.

### 5. Portable integration contracts

The kernel should expose stable contracts suitable for Rust and a reviewed C
ABI. First-class adapters should target ROS 2 and Space ROS, DDS, NASA cFS, F Prime,
Basilisk, hardware-in-the-loop systems, and embedded or real-time platforms.
Adapters translate transport and platform concepts; they do not redefine policy.

### 6. Evidence over claims

Every security and interoperability claim should be backed by executable test
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
versioned. Changes should be backward compatible where safety permits and fail
clearly where compatibility cannot be guaranteed.

### 9. Open core, sustainable ecosystem

Commercial sustainability should come from integration engineering,
mission-specific adapters, qualification evidence, certified releases,
long-term support, security maintenance, trust infrastructure, and conformance
services. The open protocol and baseline core should remain genuinely useful
without purchasing those services.

### 10. Responsible IP stewardship

Patent, trademark, contribution, and licensing decisions must preserve the
project's open interoperability goal. Any patent strategy should be established
with qualified counsel before accepting broad external contributions or making
claims about royalty-bearing use. Contributors must understand the license and
patent terms that apply to their work.

## Target product layers

1. **Open protocol specification:** identity exchange, session negotiation,
   scoped grants, replay protection, revocation, timeout, and abort behavior.
2. **Portable security kernel:** deterministic authorization and state reduction,
   independent of transports and dynamics.
3. **Open adapters:** integrations for common spacecraft, robotics, simulation,
   and flight-software frameworks.
4. **Conformance system:** test vectors, interoperability suites, fuzzing,
   negative-security tests, and reproducible reference builds.
5. **Commercial assurance:** integration, policy engineering, qualification,
   certified releases, long-term support, and operational security services.

## Strategic assets

The project should accumulate four forms of credibility:

1. **Technical maturity:** portable kernel, stable protocol, adapters, fuzzing,
   and formal analysis.
2. **Operational credibility:** realistic servicing, station-access, refueling,
   and degraded-link scenarios.
3. **Integration credibility:** cFS, F Prime, Space ROS, DDS, and C interfaces.
4. **Institutional credibility:** public governance, external contributors,
   design partners, publications, and standards participation.

The simulation serves the second asset. It should showcase complete operational
stories, including successful interactions, denial paths, degraded conditions,
recovery, and auditable outcomes. Work on visual fidelity or simulation features
is in scope only when it strengthens that operational evidence or helps intended
adopters evaluate the authorization model.

## Near-term direction

The next major milestone is a simulator-independent Rust kernel with a stable C
API, a versioned protocol draft, published test vectors, and at least two
independent adapters. The ROS 2/Gazebo environment remains the primary reference
demonstration while this portable product surface is established.
