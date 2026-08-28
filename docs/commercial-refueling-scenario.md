# Commercial Refueling Reference Scenario

## Scenario

Commercial tug **Odyssey-7**, operated by **Lunar Logistics**, requests docking
and methane refueling at **Waystation-1**, port 3. The station is operated by a
different organization and has no reason to trust the tug merely because it can
communicate over a protected link.

The station must establish four distinct facts:

1. Credential issuers were approved through station governance.
2. Odyssey-7 holds valid credentials from those issuers.
3. The party presenting them controls the encounter key for this session.
4. Each requested operation satisfies current station policy and readiness.

The visual simulation runs this happy-path scenario using deterministic mock
identity evidence. The docking motion, ROS requests, policy-shaped decisions,
Gazebo pose acknowledgements, and UI updates are live. DID resolution,
credential signatures, revocation checks, and COSE entitlement signatures are
represented but not yet cryptographically executed by the mock authority.

## Actors and Identifiers

| Actor | Example identifier | Role |
| --- | --- | --- |
| Station | `station:waystation-1` | Resource owner and policy authority |
| Station port | `station:waystation-1/port-3` | Physical and authorization audience |
| Chaser | `did:web:lunar-logistics.example:spacecraft:odyssey-7` | Credential subject |
| Encounter identity | `did:peer:2.EzOdysseyEncounter` | Pairwise session identity |
| Operator | `did:web:lunar-logistics.example` | Vehicle operator |
| Registrar | Approved issuer group | Issues registration credentials |
| Docking authority | `did:web:orbital-safety.example` | Issues interface certification |
| Local safety monitor | `station:waystation-1:navigation-monitor` | Produces readiness evidence |

These identifier values are examples, not a requirement to use `did:web`. The
station trust bundle may contain method-specific verifier profiles for multiple
DID methods or X.509 identities. No live external resolution is required during
the encounter.

## Phase 0: Out-of-Band Station Onboarding

Before Odyssey-7 approaches, station governance approves organizations that may
issue relevant credentials. Ground operations generate and sign trust bundle
`waystation-1-trust@42`, containing:

- pinned issuer verification state
- allowed credential types and schemas per issuer group
- key validity and revocation snapshots
- approved DID-method verification adapters
- policy and claim-profile versions
- activation and expiration times
- rollback-protected bundle version

The station loads the bundle into protected local storage. Staging an issuer
only permits its credentials to enter the authorization funnel. It does not
approve an individual spacecraft, session, or docking operation.

The active station policy is
[`commercial-docking-v3`](../examples/authorization/commercial-docking.policy.json).
It defaults to deny and defines the required credential, proof, session,
readiness, constraint, and entitlement rules for each transition.

## Phase 1: Service Intent and Challenge

At the rendezvous boundary, Odyssey-7 sends a signed service intent:

```json
{
  "type": "service_intent",
  "vehicle_id": "did:web:lunar-logistics.example:spacecraft:odyssey-7",
  "encounter_id": "did:peer:2.EzOdysseyEncounter",
  "station_id": "station:waystation-1",
  "port_id": "port-3",
  "mission_id": "mission:ll-2026-1842",
  "requested_services": ["dock", "methane_refuel"],
  "requested_quantity_kg": 400,
  "chaser_nonce": "chaser-381"
}
```

The station returns a challenge bound to this audience and encounter:

```json
{
  "type": "identity_challenge",
  "session_id": "session:dock-2026-1842",
  "station_id": "station:waystation-1",
  "port_id": "port-3",
  "station_nonce": "station-972",
  "required_profiles": ["registered-vehicle-v1", "idss-compatible-v1"],
  "expires_in_s": 60
}
```

In the visual simulation, the authorization funnel marks **Trust bundle**,
**Issuer scope**, and **Challenge** as these steps complete.

## Phase 2: Credential Presentation and Holder Proof

Odyssey-7 returns an arbitrary credential collection inside a bounded,
deterministically encoded presentation. Only allowlisted credential profiles can
affect policy. Unknown non-critical credentials may be retained for audit but do
not grant authority.

The useful credentials in this scenario are:

- `VehicleRegistrationCredential`, binding Odyssey-7 to Lunar Logistics
- `DockingCertificationCredential`, asserting an IDSS-compatible interface
- optionally, a service-order or payment-assurance credential for refueling

The holder proof signs the station challenge together with the vehicle,
encounter identity, station, port, mission, session, and credential digests. It
proves that a recorded credential was not copied from another spacecraft and
replayed into this encounter.

Station-controlled verifiers then produce normalized facts:

- issuer is staged for this credential type and schema
- credential signature and validity period pass
- credential status is current enough for policy
- credential subject matches Odyssey-7 or its operator as required
- required IDSS compatibility claims pass a reviewed claim profile
- holder proof is fresh, challenge-bound, and audience-bound

The chaser cannot directly assert fields such as `signature_valid` or
`issuer_trust`. Those are internal outputs of station verifiers. A representative
normalized record is
[`commercial-final-approach.input.json`](../examples/authorization/commercial-final-approach.input.json).

## Phase 3: Session Authorization and Initial Hold

After identity and eligibility checks pass, the station creates
`session:dock-2026-1842`, bound to:

- Odyssey-7's durable and encounter identities
- Waystation-1 and port 3
- mission `ll-2026-1842`
- permitted docking and refueling services
- a monotonic message sequence
- an explicit expiration and revocation state

The station's navigation monitor independently confirms the initial hold and
retreat capability. Session authorization cannot manufacture physical
readiness, and readiness cannot substitute for identity authorization.

The visual controller starts immediately so it can receive reset commands, but
holds motion for 19 seconds. During that interval, identity messages are paced
about three seconds apart and the initial hold becomes ready.

## Phase 4: Policy-Bound Docking Transitions

The existing `baseline_controller` requests transitions when Gazebo-confirmed
range reaches each checkpoint. In the visual launch, `mock_authorization`
replaces `development_gate` and applies the happy-path policy shape.

| Range and request | Security decision | Simulation effect |
| --- | --- | --- |
| `3.320 m`, `HOLD -> APPROACH` | Registration, holder proof, session context, and initial hold pass | Single-use `enter_approach` entitlement is consumed; chaser starts moving |
| `1.120 m`, `APPROACH -> FINAL_APPROACH` | Identity, IDSS credential, authorized session, corridor and closing-rate checks pass | `enter_final_approach` entitlement is consumed |
| `0.320 m`, `FINAL_APPROACH -> SOFT_CAPTURE` | Interface compatibility, session, alignment, and capture readiness pass | `engage_soft_capture` entitlement is consumed |
| `0.040 m`, `SOFT_CAPTURE -> HARD_DOCK` | Soft capture is confirmed, latches are ready, relative motion is stable | Ten-second, single-use `engage_hard_dock` entitlement is consumed |
| `0.000 m`, hard dock complete | Gazebo acknowledges the final pose | UI reports hard dock; no additional movement entitlement exists |

The visual mock records `ALLOW_POLICY_SATISFIED`, a stage evidence summary, and
a unique entitlement ID. The complete production decision contract additionally
records policy and trust-bundle versions, evidence digests, and obligations. A
complete allow example is
[`commercial-final-approach.allow.json`](../examples/authorization/commercial-final-approach.allow.json).

If a required fact fails, no entitlement is created. For example, an expired
docking credential produces `DENY_CREDENTIAL_EXPIRED`, and the controller
remains at the current checkpoint. See
[`expired-credential.deny.json`](../examples/authorization/expired-credential.deny.json).

## Phase 5: Service Authorization After Docking

Docking does not imply permission to use station resources. A production system
would perform another policy decision before opening the methane interface:

```json
{
  "action": "transfer_resource",
  "resource_type": "cryogenic_methane",
  "maximum_quantity_kg": 400,
  "station_id": "station:waystation-1",
  "port_id": "port-3",
  "session_id": "session:dock-2026-1842",
  "expires_in_s": 300,
  "single_use": true
}
```

The service gate would additionally verify valve state, pressure compatibility,
commercial clearance, metering readiness, and emergency shutdown availability.
Transfer receipts would record the measured quantity rather than the maximum
authorized quantity.

The current simulation stops at hard dock; resource transfer is documented as
the next service-gate extension.

## Phase 6: Audit, Receipt, and Departure

The station retains a signed audit package containing:

- message and credential digests, not unnecessary private claims
- verification and revocation results
- exact trust bundle and policy versions
- holder-proof challenge and session bindings
- allow, deny, and indeterminate decisions with reason codes
- entitlement issuance and atomic consumption records
- readiness-evidence digests and timestamps
- docking and service completion receipts
- aborts, retries, expiry, and operator interventions

The same pattern authorizes disconnect and departure. Completed sessions are
closed, outstanding entitlements are revoked, and replay identifiers remain in
durable storage for their required retention period.

## Running the Visual Scenario

Start the three-session visual pool:

```powershell
docker compose up --build docking-gateway
```

Open [http://localhost:8080](http://localhost:8080). The gateway assigns each
browser to an isolated ROS domain and Gazebo partition for the duration of its
idle lease. The dashboard displays live state and range, authorization checks,
protocol messages, local evidence, issued entitlements, and replay data.

Choose a **Run profile**, then select **Start simulation**. The flight recorder
becomes available after successful hard dock or an authorization denial.
Available profiles are:

| Profile | Denied gate | Evidence shown |
| --- | --- | --- |
| Nominal authorization | None | All four single-use entitlements are consumed |
| Expired vehicle credential | Enter approach at `3.320 m` | Credential expiration, verifier time, and allowed clock skew |
| Approach corridor violation | Enter final approach at `1.120 m` | Cross-track error, limit, closing rate, and corridor ID |
| Latch telemetry incomplete | Engage hard dock at `0.040 m` | Ready-latch count, ring load, relative rate, and required count |

Failure profiles are deterministic. A denial creates no entitlement and halts
the controller at the corresponding checkpoint.

## Implementation Boundary

The visual flow is deliberately realistic in structure and deliberately mock in
cryptographic execution. The production replacement must:

1. Load signed trust bundles and policy files.
2. Resolve only staged method profiles from local cached material.
3. Verify real credential and holder-proof signatures.
4. Build the normalized evaluation record using station-owned facts.
5. Evaluate the declarative stage policy in Rust.
6. Sign deterministic CBOR entitlements in COSE.
7. Persist replay and entitlement-consumption state.
8. Publish the existing ROS transition decision only after enforcement succeeds.

That replacement preserves the current controller, Gazebo world, dashboard
status contract, and physical checkpoints.