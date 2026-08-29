# ACCESS

**Access Control and Credential Evaluation for Space Systems**

Open, portable, fail-closed authorization for safety-critical interactions
between spacecraft, stations, servicing vehicles, and robotic systems.

## Overview

The system answers one operational question:

> May this independently operated vehicle perform this safety-critical action,
> here, now, under these constraints?

Verified identity, operator policy, encounter state, and local safety evidence
produce an auditable decision. Allowed actions receive short-lived, narrowly
scoped, replay-resistant entitlements enforced at the protected transition or
actuation boundary. Missing, stale, revoked, replayed, or unverifiable authority
fails closed according to mission policy.

The first reference use case covers visiting-vehicle progression through
rendezvous, proximity operations, and docking. The same authorization model can
be applied to servicing, refueling, capture, assembly, and resource access.

## Components

1. **Open protocol:** identity, session, authority, revocation, and failure
	semantics.
2. **Portable kernel:** deterministic, simulator-independent authorization and
	state reduction.
3. **Integration contracts:** stable Rust and C interfaces plus adapters for
	flight, robotics, and simulation frameworks.
4. **Conformance evidence:** schemas, test vectors, negative tests, threat
	models, and reproducible builds.

## Status

The repository currently has two implemented slices:

1. `docking_identity_core`, a physics-independent Rust security crate.
2. A containerized ROS 2 Humble and Gazebo Fortress reference environment.

The Rust core is connected as the live ROS transition authority through a
JSON-lines process adapter. The reference flow performs Ed25519/COSE identity,
credential, holder-proof, session, and stage-grant operations. This project is
not flight-qualified.

## Docking reference environment

The simulation provides:

- a zero-gravity Fortress world with chaser and target spacecraft
- collision-enabled bodies and visible docking interfaces
- a deterministic kinematic approach controller backed by Gazebo's pose service
- explicit `HOLD`, `APPROACH`, `FINAL_APPROACH`, `SOFT_CAPTURE`, and `HARD_DOCK` states
- a Rust ACCESS authority at the protected transition boundary
- ROS status, transition-request, and transition-decision topics
- a headless end-to-end smoke test

Build and start the baseline in idle `HOLD`:

```powershell
docker compose build docking-sim
docker compose up docking-sim
```

The simulation does not begin automatically. Trigger a run explicitly from
another terminal:

```powershell
docker compose exec docking-sim ros2 topic pub --once /docking/reset std_msgs/msg/Empty "{}"
```

In another terminal, inspect its state:

```powershell
docker compose exec docking-sim ros2 topic echo /docking/status
```

Run the self-terminating smoke test:

```powershell
./scripts/smoke_test.ps1
```

The expected terminal state is `HARD_DOCK_REACHED`. The test advances simulated
range only after Gazebo acknowledges each pose update.

### Browser visualization

Start the three-session visual simulation pool and gateway:

```powershell
docker compose up --build docking-gateway
```

Open [http://localhost:8080](http://localhost:8080) and select **Start
simulation**. Each browser is assigned one of three isolated ROS/Gazebo
instances. A fourth session receives HTTP `503` until a slot is released or an
abandoned lease expires after 60 seconds.

Select **Start simulation** in the dashboard to reset the chaser and development
gate and begin the selected profile. Opening or refreshing the dashboard never
starts a run. The display remains in `RESETTING` until Gazebo confirms the
initial pose, shows a brief `HOLD`, and then starts the docking sequence.

You can also start a visual slot directly from the command line:

```powershell
docker compose exec docking-visual-1 ros2 topic pub --once /docking/reset std_msgs/msg/Empty "{}"
```

You can recreate the complete visual pool from the command line:

```powershell
docker compose restart docking-visual-1 docking-visual-2 docking-visual-3 docking-gateway
```

To expose the pool through ngrok, keep an ignored `.env` file at the repository
root containing `ngrok_authtoken=<token>`, then run:

```powershell
docker compose up -d ngrok
```

The tunnel does not add authentication. Anyone with its URL can claim a
simulation slot and trigger a run.

Stop the simulation with `Ctrl+C`, then remove its container:

```powershell
docker compose down
```

The Rust core provides:

- tagged COSE_Sign1 envelopes with canonical CBOR claims
- Ed25519 signing and strict verification
- protected key IDs, issuer and recipient binding, freshness checks, and replay rejection
- deterministic, fail-closed gates for final approach, soft capture, and hard dock

Runtime configuration is selected with `ACCESS_AUTHORITY_COMMAND`,
`ACCESS_SIGNER_COMMAND`, `ACCESS_IDENTITIES_FILE`, `ACCESS_TRUST_BUNDLE_FILE`,
`ACCESS_POLICY_FILE`, and `ACCESS_STATE_DIR`. The checked-in files under
`config/access/` are public simulation fixtures, including deliberately exposed
private seeds. See [Security configuration](docs/security-configuration.md)
before substituting deployment credentials.

Run the Rust tests on any host with Rust:

```bash
cargo test --workspace
```

## Documentation

- [Overview and terminology](docs/positioning.md)
- [Design principles](docs/project-principles.md)
- [Architecture](docs/architecture.md)
- [Authorization policy](docs/authorization-policy.md)
- [Security configuration](docs/security-configuration.md)
- [Commercial refueling scenario](docs/commercial-refueling-scenario.md)
- [Roadmap](docs/roadmap.md)

### GPU override

The current headless smoke test does not require a GPU. For future camera,
lidar, and GUI rendering workloads on an NVIDIA Docker host, apply the override:

```powershell
docker compose -f compose.yaml -f compose.gpu.yaml up docking-sim
```

### Reference fidelity

The reference environment uses deterministic kinematic motion through Gazebo's
`SetEntityPose` service. It validates world loading, ROS/Gazebo transport,
spacecraft motion, state sequencing, and the authorization insertion point. It
does not yet model thrusters, six-degree-of-freedom GNC, compliant contact,
capture latches, or hard-dock constraints.

## Security status

This is a development baseline, not flight-certified software. Production and
hardware-in-the-loop deployments must add protected key storage, durable replay
state, certificate lifecycle management, SROS 2 enclaves, secure boot, audit
retention, independent hazard analysis, and qualification evidence.