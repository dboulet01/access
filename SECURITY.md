# Security Policy

## Supported versions

ACCESS is currently an executable reference model. No released version is
represented as flight certified or approved for operational spacecraft use.
Security fixes target the latest revision on the default branch until a formal
release-support policy is published.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability involving signature or
credential verification, authorization bypass, replay protection, entitlement
consumption, policy integrity, key handling, or protected-state enforcement.
Use the repository host's private security-advisory mechanism and include:

- affected revision and component
- reproduction steps or a minimal test vector
- expected and observed behavior
- security impact and prerequisites
- any proposed mitigation

Do not include operational credentials, private keys, tokens, mission data, or
information from systems you are not authorized to test.

The maintainers should acknowledge a complete report within five business days,
coordinate validation and remediation privately, and publish an advisory after
a fix and affected-version assessment are available. This target is not a
service-level guarantee.

## Deployment warning

Files under `config/access` contain public simulation fixtures. The reference
signer, file journals, Docker deployment, ROS graph, dashboard, gateway, and
ngrok path are not production security controls. Operational integrations must
provide protected keys, authenticated configuration, trusted time, qualified
entropy, rollback-resistant state, secure transport, audit retention, platform
hardening, and mission-specific assurance.
