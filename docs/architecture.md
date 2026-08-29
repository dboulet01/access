# ACCESS Architecture

**Access Control and Credential Evaluation for Space Systems**

## System overview

```mermaid
flowchart LR
    PEER[Independent peer] --> TRANSPORT[Existing secure transport]
    TRANSPORT --> ADAPTER[Platform adapter]
    ADAPTER --> CORE[Portable authorization kernel]
    POLICY[Operator policy] --> CORE
    SAFETY[Local safety evidence] --> CORE
    CORE --> GATE[Protected transition gate]
    GNC[Mission GNC] --> GATE
    GATE --> ACT[Actuation or resource access]
    CORE --> AUDIT[Auditable decision evidence]
```

Secure transport protects message delivery and is assumed to be pre-established
by mission operations. ACCESS runs above that channel and determines whether the
authenticated peer has current, scoped authority for the requested interaction.
Mission GNC retains responsibility for determining whether motion is physically
safe; authorization cannot override failed local safety evidence.

The portable boundary includes protocol claims, policy evaluation, replay state,
entitlement issuance and consumption, and deterministic failure behavior. ROS 2,
DDS, cFS, F Prime, network transports, key stores, clocks, dynamics, and
actuators connect through adapters.

## Reference environment

Python coordinates the reference simulation. The Rust authorization core is
responsible for protected transition decisions. Dynamics and hardware adapters
remain outside the authorization protocol.

```mermaid
flowchart LR
    PY[Python orchestration] --> ROS[ROS 2 graph]
    GZ[Gazebo Fortress] <--> BR[ros_gz bridge]
    BR <--> ROS
    BAS[Basilisk adapter] <--> ROS
    ROS --> ID[Rust identity node]
    ID --> CORE[Rust security core]
    CORE --> GATE[Rust transition gate]
    GATE --> ACT[Actuator command adapter]
    ACT --> GZ
    ACT --> HIL[Future HIL]
```

The actuator adapter accepts protected state transitions only from the Rust gate.
An authorization-status topic is telemetry, not authority. Production systems
must also use SROS 2 governance and enclave permissions to prevent another ROS
participant from impersonating the gate at the DDS layer.

## Package status

| Package | Status | Responsibility |
| --- | --- | --- |
| `docking_identity_core` | Implemented | COSE/CBOR, credentials, sessions, grants, replay defense, deterministic reducer |
| `docking_interfaces` | Implemented | Backend-neutral state, request, readiness, and decision wire contracts |
| `docking_orchestration` | Implemented | ROS adapter, readiness monitor, launch, controller, dashboard, smoke monitor |
| `docking_gazebo` | Implemented baseline | Zero-gravity Fortress world and spacecraft instances |
| `docking_description` | Implemented baseline | Reusable URDF/Xacro spacecraft description |
| `access-authority` | Implemented reference | Rust policy authority using public trust and durable replay state |
| `access-signer` | Implemented fixture | Role-bound signing process; replaceable with HSM/KMS integration |
| `docking_basilisk_bridge` | Planned | Optional high-fidelity dynamics bridge |
| `docking_capture_plugin` | Planned | Compliant contact and capture constraints |

## Current ROS graph

```mermaid
flowchart LR
    CTRL[baseline_controller] -->|TransitionRequest| ADAPTER[access_authorization]
    CTRL -->|DockingStatus| READY[readiness_monitor]
    READY -->|ReadinessEvidence| ADAPTER
    ADAPTER -->|JSON-lines request| ACCESS[Rust access-authority]
    ACCESS -->|Role-bound signing request| SIGNER[access-signer or HSM/KMS]
    ACCESS -->|Verified decision| ADAPTER
    ADAPTER -->|TransitionDecision| CTRL
    CTRL -->|SetEntityPose| BRIDGE[ros_gz_bridge]
    BRIDGE --> GZ[Gazebo Fortress]
    CTRL -->|DockingStatus| MON[docking_monitor or telemetry]
```

The Python adapter layer now separates chaser and station ACCESS participants
into distinct nodes and explicit protocol topics. The station-side Rust process
owns session and durable replay state, evaluates policy, issues and consumes
grants, invokes the reducer, and returns authoritative audit metadata.

For standards-aligned credential transport and profile details, see
[credential-exchange-profile.md](credential-exchange-profile.md).

The normalized policy input, structured decision, and single-use entitlement
contracts are defined in [authorization-policy.md](authorization-policy.md).

Run only one transition authority. DDS/SROS 2 policy must ensure only the
selected adapter can publish transition decisions.

## Identity topic contract

All three topics carry an opaque tagged COSE_Sign1 byte string. Routing metadata
may be duplicated in ROS fields for observability, but only signed CBOR claims
are authoritative.

| Topic | Direction | Signed message |
| --- | --- | --- |
| `/identity/request` | chaser to target | identity request with nonce and capabilities |
| `/identity/session` | target to chaser | session offer/challenge bound to request nonce |
| `/identity/authorize` | bidirectional | session proof or stage-limited grant |

Recommended QoS is reliable, keep-last 16, volatile durability, and a finite
lifespan shorter than the protocol freshness window. Each participant verifies
signature, issuer, recipient, freshness, nonce uniqueness, session binding, and
message type before changing local state.

```mermaid
sequenceDiagram
    participant C as Chaser Rust node
    participant T as Target Rust node
    participant G as Target transition gate
    C->>T: /identity/request (signed request + Nc)
    T->>C: /identity/session (signed session + Nc + Nt)
    C->>T: /identity/authorize (signed proof + Nt)
    T->>C: /identity/authorize (signed stage grant)
    C->>G: requested transition + verified local grant
    G-->>C: approved transition or fail-closed denial
```

## Required transition policy

| Requested state | Required evidence |
| --- | --- |
| `FINAL_APPROACH` | peer identity verified |
| `SOFT_CAPTURE` | negotiated session authorized |
| `HARD_DOCK` | explicit, unexpired docking authorization |

Abort remains available from every state and does not require authorization.
Authorization is monotonic only within one session; revocation, timeout, peer
change, or clock failure returns the gate to `HOLD` or `ABORTED` according to the
mission safety policy.

## Compatibility strategy

NASA Space ROS, OpenAMR, microgravity simulators, and Basilisk integrate through
ROS adapters and canonical state/command topics. Their native state machines do
not become trusted authorities. For the Jazzy-based OpenAMR stack, use a separate
Jazzy container and DDS-compatible IDL at the boundary instead of mixing Jazzy
packages into the Humble/Fortress image.
