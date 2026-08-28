# Secure Docking Simulation

Modular ROS 2 and Gazebo environment for rendezvous, proximity operations,
docking, and berthing with cryptographically enforced transition authority.

## Working baseline

The repository now has two validated vertical slices:

1. A containerized ROS 2 Humble and Gazebo Fortress docking simulation.
2. `docking_identity_core`, a physics-independent Rust security crate.

The simulation provides:

- a zero-gravity Fortress world with chaser and target spacecraft
- collision-enabled bodies and visible docking interfaces
- a deterministic kinematic approach controller backed by Gazebo's pose service
- explicit `HOLD`, `APPROACH`, `FINAL_APPROACH`, `SOFT_CAPTURE`, and `HARD_DOCK` states
- a separate development transition gate that can be replaced by the Rust identity node
- ROS status, transition-request, and transition-decision topics
- a headless end-to-end smoke test

Build and run the continuous baseline:

```powershell
docker compose build docking-sim
docker compose up docking-sim
```

In another terminal, inspect its state:

```powershell
docker compose exec docking-sim ros2 topic echo /docking/status
```

Run with human-readable activity logs for status, range, transition requests,
and authority decisions. This continuous mode remains available at hard dock
until stopped with `Ctrl+C`:

```powershell
docker compose run --rm docking-sim ros2 launch docking_orchestration baseline_sim.launch.py verbose:=true
```

To show the same verbose output and exit automatically after confirmed hard
dock, enable the smoke monitor too:

```powershell
docker compose run --rm docking-sim ros2 launch docking_orchestration baseline_sim.launch.py verbose:=true smoke_test:=true
```

The telemetry interval defaults to 0.5 seconds and can be changed without
increasing the controller's own logging:

```powershell
docker compose run --rm docking-sim ros2 launch docking_orchestration baseline_sim.launch.py verbose:=true telemetry_interval:=0.2
```

Run the self-terminating smoke test:

```powershell
./scripts/smoke_test.ps1
```

The expected terminal state is `HARD_DOCK_REACHED`. The test advances simulated
range only after Gazebo acknowledges each pose update.

## Browser visualization

Start the three-session visual simulation pool and gateway:

```powershell
docker compose up --build docking-gateway
```

Open [http://localhost:8080](http://localhost:8080). The launch waits eight
seconds before motion and uses a slower approach speed so the sequence is easy
to follow. The display is driven by live ROS status, request, and decision
topics from the same Gazebo-backed process used by the headless test.
Each browser receives an HTTP-only session cookie and is pinned to one of three
visual services. Each service has its own ROS domain and Gazebo partition, so
three people can run different profiles concurrently without sharing state or
transition decisions. A fourth new session receives HTTP `503` until a
30-minute idle lease expires or the gateway is restarted.

Select **Rerun simulation** in the dashboard to reset the chaser and development
gate. The display remains in `RESETTING` until Gazebo confirms the initial pose,
shows a three-second `HOLD`, and then starts a new docking sequence.

You can recreate the complete visual pool from the command line:

```powershell
docker compose restart docking-visual-1 docking-visual-2 docking-visual-3 docking-gateway
```

To expose the pool through ngrok, keep an ignored `.env` file at the repository
root containing `ngrok_authtoken=<token>`, then run:

```powershell
docker compose up -d ngrok
```

The configured public URL is
[https://unisotropous-overmellow-genevieve.ngrok-free.dev](https://unisotropous-overmellow-genevieve.ngrok-free.dev).
On ngrok's free tier, each browser must confirm the **Visit Site** warning once
before the dashboard loads.
The tunnel is intended for temporary demonstrations and does not add user
authentication; anyone with the URL can claim a simulation slot and trigger a
run. The ngrok inspector is available only locally at
[http://localhost:4040](http://localhost:4040).

Stop the simulation with `Ctrl+C`, then remove its container:

```powershell
docker compose down
```

The Rust core provides:

- tagged COSE_Sign1 envelopes with canonical CBOR claims
- Ed25519 signing and strict verification
- protected key IDs, issuer and recipient binding, freshness checks, and replay rejection
- deterministic, fail-closed gates for final approach, soft capture, and hard dock

Run the Rust tests on any host with Rust:

```bash
cargo test --workspace
```

The ROS 2/Gazebo runtime is containerized. Python owns launch, simulation
composition, the baseline kinematic adapter, visualization, and future Basilisk adapters.
Rust will own identity, authorization, protected docking transitions, and the
last software gate before actuator commands.

See [docs/architecture.md](docs/architecture.md) for package boundaries and the
authorization replacement boundary.

See [docs/authorization-policy.md](docs/authorization-policy.md) for the
policy-bound decision model, schemas, entitlement semantics, and simulation
mapping.

See [docs/commercial-refueling-scenario.md](docs/commercial-refueling-scenario.md)
for a complete practical run from organization onboarding through credential
proof, docking entitlements, servicing authorization, and audit.

## GPU override

The current headless smoke test does not require a GPU. For future camera,
lidar, and GUI rendering workloads on an NVIDIA Docker host, apply the override:

```powershell
docker compose -f compose.yaml -f compose.gpu.yaml up docking-sim
```

## Current fidelity

This baseline deliberately uses deterministic kinematic motion through Gazebo's
`SetEntityPose` service. It validates world loading, ROS/Gazebo transport,
spacecraft motion, state sequencing, and the authorization insertion point. It
does not yet model thrusters, six-degree-of-freedom GNC, compliant contact,
capture latches, or hard-dock constraints. Those belong behind the same
controller and transition interfaces.

## Security status

This is a development baseline, not flight-certified software. Production and
hardware-in-the-loop deployments must add protected key storage, durable replay
state, certificate lifecycle management, SROS 2 enclaves, secure boot, audit
retention, independent hazard analysis, and qualification evidence.