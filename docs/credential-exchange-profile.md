# Credential Exchange Profile

## Scope

This profile defines how ACCESS exchanges credential material between separate
chaser and station nodes while assuming a pre-established secure transport
channel.

ACCESS is an application-layer authorization protocol. It does not establish or
manage the communications channel. The mission communications layer is assumed
to provide confidentiality, integrity, peer authentication, and anti-downgrade
protection for transport sessions.

## Transport assumption

- secure channel is pre-established before ACCESS exchange begins
- examples: mission-provided secure data link, mutually authenticated TLS, or
  equivalent operational channel controls
- ACCESS still performs message-level signatures, challenge binding, and replay
  protection for authorization semantics

## Runtime separation

- ACCESS Requester (AR): constructs and transmits access and transition requests
- ACCESS Authority (AA): validates messages and issues decisions
- ACCESS Policy Engine (APE): evaluates policy and evidence for AA
- ACCESS Enforcement Point (AEP): consumes issued decisions or grants before
  protected transitions

No shared in-memory ACCESS session is assumed between chaser and station.
Session state, replay state, and policy state are owned by station runtime.

## Standard flow selection

ACCESS standard interactions are defined in
[access-protocol-flows.md](access-protocol-flows.md).

This profile currently selects flow SF1 (Encounter Authorization).

## W3C VC 2.0 credential artifacts

The simulation uses these profiles:

- Vehicle registration credential:
  [schemas/vc-vehicle-registration-credential.schema.json](../schemas/vc-vehicle-registration-credential.schema.json)
- Docking certification credential:
  [schemas/vc-docking-certification-credential.schema.json](../schemas/vc-docking-certification-credential.schema.json)
- Credential presentation envelope:
  [schemas/vc-access-presentation.schema.json](../schemas/vc-access-presentation.schema.json)

These schemas align to VC 2.0 core object model and provide required claims for
station policy evaluation.

## ACCESS protocol exchange sequence

1. ACCESS Requester sends access_request to ACCESS Authority with scenario and presentation
   profile intent.
2. ACCESS Authority validates request context and establishes session state through ACCESS Policy Engine.
3. ACCESS Authority issues session authorization response to ACCESS Requester.
4. ACCESS Requester sends transition request for each protected stage advance.
5. ACCESS Authority evaluates policy using station-local readiness and verified
   credentials.
6. ACCESS Authority returns transition decision and, for allow outcomes, issues and
  consumes a stage-bound grant at ACCESS Enforcement Point.

## Credential material exchange binding

Current simulator profile (reference mode):

- credential artifacts are not carried over chaser/station ACCESS topic
  messages
- `access_request` carries presentation profile intent only
- station authority loads deterministic fixture-backed credential material for
  evaluation during session establishment

Production profile (explicit mode):

- chaser sends `access_presentation` containing credential material and holder
  proof
- station verifies proofs and supplies normalized facts to policy evaluation
  during session establishment

## Station decision contract

Station outcomes continue to map into:

- authorization status telemetry
- transition decision message for controller integration
- policy assessments and reason codes for auditability

## Interoperability notes

- The VC profile is intentionally strict for simulator determinism.
- Production deployments may use VC-JOSE-COSE or Data Integrity proof suites so
  long as verifier behavior and policy inputs remain equivalent.
- Unknown credentials may be retained for audit but only configured credential
  profiles influence allow decisions.
