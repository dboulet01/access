# ACCESS

**Access Control and Credential Exchange for Space Systems**

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

ACCESS is an application-level **spacecraft interaction authorization** system.
It complements secure communications, mission safety logic, flight software,
and mechanical interface standards; it does not replace them. The protocol and
security core are independent of ROS 2, Gazebo, Python, and the dashboard.

## Goals and principles

- **Open and portable:** independent implementations can use the protocol and
	core without the reference simulator.
- **Small trusted core:** authentication, policy, entitlement, replay, and
	protected-state logic remain deterministic and auditable.
- **Fail closed:** stale, missing, replayed, or unverifiable evidence never
	releases a protected action.
- **Verified facts:** the authority authenticates evidence before the ACCESS
	Policy Service evaluates it.
- **Explicit boundaries:** adapters translate platforms and transports without
	redefining authorization semantics.
- **Evidence-backed maturity:** tests, vectors, schemas, threat analysis, and
	qualification evidence support claims; the reference is not flight-certified.

Canonical roles, interfaces, and component mappings are defined only in
[Architecture](docs/architecture.md).

## Status

The repository currently has two implemented slices:

1. `access_core`, a physics-independent Rust security crate.
2. A containerized ROS 2 Humble and Gazebo Fortress executable integration test
	suite.

The live reference uses Rust for authentication, session integrity, entitlement
handling, policy evaluation, and enforcement; and ROS 2/Gazebo
for integration and simulation. The client independently verifies returned
entitlements. See [Architecture](docs/architecture.md) for the role mapping.

## Production product boundary

The ACCESS product is the portable authentication and authorization stack plus
integration interfaces for existing flight software. Its intended product
boundary contains protocol verification, credential and trust evaluation,
policy decisions, narrowly scoped entitlements, replay defense, protected-state
enforcement, and adapters that connect those functions to mission software.

ROS 2, Gazebo, the docking controller, the web dashboard, the session gateway,
Docker Compose, and the commercial docking scenario are test suites and test
fixtures. They exist only to exercise ACCESS in a live, observable system and
prove integration behavior; they are not deliverable flight GNC, vehicle
dynamics, mission communications, or operational deployment software.

The repository currently provides evidence for an executable reference model.
It does not claim flight certification or flight readiness. The controlled work
and evidence required before such a claim are tracked in
[Flight readiness](docs/flight-readiness.md) and [Test strategy](docs/test-strategy.md).

## Executable docking integration test suite

The test suite provides:

- a zero-gravity Fortress world with chaser and target spacecraft
- collision-enabled bodies and visible docking interfaces
- a deterministic kinematic approach controller backed by Gazebo's pose service
- explicit `HOLD`, `APPROACH`, `FINAL_APPROACH`, `SOFT_CAPTURE`, and `HARD_DOCK` states
- a Rust ACCESS authority at the protected transition boundary
- ground-managed ACCESS authorization policy evaluated over verified facts
- client-side authority, audience, session, stage, expiry, and replay checks
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

### Browser test visualization

Start the three-session visual simulation pool and gateway:

```powershell
docker compose up --build docking-gateway
```

Open [http://localhost:8080](http://localhost:8080) and select **Start
simulation**. Each browser is assigned one of three isolated ROS/Gazebo
instances. A fourth session receives HTTP `503` until a slot is released or an
abandoned lease expires after 60 seconds.

Select **Start simulation** in the dashboard to reset the chaser and authority
session and begin the selected scenario. Opening or refreshing the dashboard never
starts a run. The display remains in `RESETTING` until Gazebo confirms the
initial pose, shows a brief `HOLD`, and then starts the docking sequence.

You can also start a visual slot directly from the command line:

```powershell
docker compose exec docking-visual-1 ros2 topic pub --once /docking/reset std_msgs/msg/Empty "{}"
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

Runtime settings and fixture warnings are documented in
[Security configuration](docs/security-configuration.md). Checked-in identities
and Trust Bundles are public simulation material, not deployment credentials.

Run the Rust tests on any host with Rust:

```bash
cargo test --workspace
```

## Documentation guide

For a concise, stakeholder-facing introduction, open the
[ACCESS visual flyer](docs/access-flyer.html) or download the
[scrollable e-flyer PDF](docs/ACCESS-project-flyer.pdf).

The documentation has one source for each concern:

1. [Architecture](docs/architecture.md) — canonical roles, interfaces, trust
	boundaries, and implementation mapping.
2. [Protocol flows](docs/access-protocol-flows.md) — peer messages, invariants,
	credential carriage, and implemented/planned flows.
3. [Authorization](docs/authorization-policy.md) — policy evaluation,
	entitlements, provenance, and reference engine configuration.
4. [Security configuration](docs/security-configuration.md) — Trust Bundles,
	runtime settings, key boundaries, and production requirements.
5. [Commercial refueling scenario](docs/commercial-refueling-scenario.md) — the
	end-to-end reference use case.
6. [Roadmap](docs/roadmap.md) — current maturity and planned work.
7. [Flight readiness](docs/flight-readiness.md) — product boundary, assurance
	workstreams, evidence gates, and release criteria.
8. [Test strategy](docs/test-strategy.md) — current verification and required
	qualification evidence.
9. [Threat model](docs/threat-model.md) — security objectives, trust boundaries,
	misuse cases, controls, and residual risks.
10. [Assurance requirements](docs/assurance-requirements.md) — stable production
	requirement identifiers and evidence status.

### GPU override

The current headless smoke test does not require a GPU. For future camera,
lidar, and GUI rendering workloads on an NVIDIA Docker host, apply the override:

```powershell
docker compose -f compose.yaml -f compose.gpu.yaml up docking-sim
```

### Test-suite fidelity

The executable test environment uses deterministic kinematic motion through Gazebo's
`SetEntityPose` service. It validates world loading, ROS/Gazebo transport,
spacecraft motion, state sequencing, and the authorization insertion point. It
does not yet model thrusters, six-degree-of-freedom GNC, compliant contact,
capture latches, or hard-dock constraints.

## Security status

This is a development baseline, not flight-certified software. Production and
hardware-in-the-loop deployments must add protected key storage,
rollback-resistant replay storage, authenticated configuration distribution,
certificate lifecycle management, SROS 2 enclaves, secure boot, audit retention,
independent hazard analysis, and qualification evidence.