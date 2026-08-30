# ACCESS Threat Model

## Scope

This threat model covers the production ACCESS authentication and authorization
core and its integration boundaries. The executable ROS 2/Gazebo environment is
a test suite used to exercise these controls; it is not the deployment model.

## Security objectives

1. Only authenticated, currently trusted principals can establish a session.
2. Only policy-permitted actions with fresh mandatory local evidence can receive
   an entitlement.
3. Every entitlement is integrity protected and bound to authority, recipient,
   session, action, policy provenance, validity window, and unique identifier.
4. An entitlement releases at most one protected action.
5. Missing, malformed, stale, replayed, unknown, or unverifiable inputs fail
   closed.
6. Security-relevant decisions and state changes are attributable and auditable.

## Trust boundaries

| Boundary | Untrusted input | Required production control |
| --- | --- | --- |
| Mission transport to AC/AAS | Peer messages, ordering, duplication, delay | Authenticated secure transport plus ACCESS signature, audience, freshness, session, and replay checks |
| Credential and trust configuration | Bundles, policy, versions, key status | Signed distribution, authorized activation, monotonic versions, rollback prevention, staged rotation |
| AAS to APS | Normalized authorization facts | Only facts established by authentication code; typed contract; deny on missing facts |
| AEP to AAS/AEG | Readiness and safety evidence | Authenticated local source, freshness, quality, provenance, mandatory-condition enforcement |
| AAS/AEG to protected action | Entitlement and action request | Atomic verify-and-consume, local condition recheck, single authority owner |
| Core to platform services | Time, entropy, signing, durable state, audit | Qualified adapters with explicit failure, deadline, recovery, and health contracts |
| Operator and maintenance interfaces | Configuration and recovery commands | Strong authentication, least privilege, two-person controls where required, immutable audit |

## Threats, controls, and open work

| ID | Threat | Current control | Required production evidence or mitigation |
| --- | --- | --- | --- |
| TM-01 | Forged peer, credential, or entitlement | Ed25519 COSE verification and scoped trust stores | Independent cryptographic review, canonical vectors, algorithm-agility policy |
| TM-02 | Message or entitlement replay | Nonces, grant identifiers, replay caches, single-use consumption | Rollback-resistant backend, power-loss and state-restore testing |
| TM-03 | Cross-session, audience, or action substitution | Recipient, session, stage, rule, and grant binding | Negative conformance vectors and property tests |
| TM-04 | Stale credential, trust, policy, evidence, or grant | Validity and freshness checks | Trusted-clock contract, degraded-clock state machine, boundary tests |
| TM-05 | Configuration rollback or unauthorized policy change | Version and validity checks in reference core | Signed manifests, monotonic protected counter, activation authorization and audit |
| TM-06 | Compromised signing key | Role-separated signer process in tests | HSM or secure-element adapter, rotation, revocation, compromise recovery |
| TM-07 | Entropy failure or predictable identifiers | OS entropy and injectable `RandomSource`; errors deny | Qualified entropy adapter, health monitoring, fault injection |
| TM-08 | Durable-state corruption or write failure | Synchronized append, corrupt-load rejection, commit-before-accept | Qualified backend with integrity, rollback detection, bounded growth, recovery tests |
| TM-09 | Policy engine bypass or fact injection | APS receives normalized verified facts; reducer remains fail closed | Typed ABI, data-flow review, fuzzing, policy mutation and differential tests |
| TM-10 | Evidence-provider spoofing | Logical AEP boundary and freshness policy | Platform authentication, source allowlist, quality flags, enforcement-time recheck |
| TM-11 | Local decision publisher spoofing | Documented single-authority rule | Flight-framework permissions or SROS 2 test policy; direct AEG-to-actuator binding |
| TM-12 | Denial of service or resource exhaustion | Deadlines in reference subprocess adapters | Bounded parsing, queues, memory, CPU and journal growth; load and WCET evidence |
| TM-13 | Downgrade or incompatible protocol interpretation | Versioned profiles | Explicit negotiation, minimum versions, downgrade rejection vectors |
| TM-14 | Audit deletion or repudiation | Structured events in reference implementation | Protected append-only audit sink, retention, clock provenance, export and review |
| TM-15 | Supply-chain compromise | Locked Rust dependencies and CI | Dependency policy, SBOM, provenance, reproducible builds, signed releases |

## Misuse cases

- An authenticated vehicle requests an action outside its credential or policy
  scope. ACCESS must deny without relying on transport identity alone.
- An operator activates an older but correctly signed policy. Activation must be
  rejected by protected monotonic state.
- A valid entitlement is restored with an old filesystem snapshot. The
  production backend must detect rollback and deny.
- Mission time steps backward or loses synchronization. New sessions and grants
  must be withheld according to the configured degraded-clock policy.
- The policy service, signer, evidence provider, persistence service, or adapter
  misses its deadline. The enforcement point must remain closed.
- A test fixture or deterministic key is supplied to a production profile.
  Configuration activation must reject the fixture classification.

## Residual risk and ownership

ACCESS authorization cannot make an unsafe trajectory safe and cannot establish
facts that its platform adapters do not authenticate. Mission engineering owns
hazard analysis, safe-state behavior, GNC, transport security, hardware
qualification, and operational recovery. Each integration must map these threats
to system hazards and close or formally accept residual risk before deployment.

This model must be reviewed whenever a protocol field, trust boundary, adapter,
cryptographic algorithm, persistence backend, or protected action changes.
