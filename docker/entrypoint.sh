#!/usr/bin/env bash
set -euo pipefail

set +u
source /opt/ros/humble/setup.bash
source /workspace/install/setup.bash
set -u

required_config=(
	"${ACCESS_IDENTITIES_FILE:-/workspace/config/access/simulation-identities.json}"
	"${ACCESS_TRUST_BUNDLE_FILE:-/workspace/config/access/simulation-trust-bundle.json}"
	"${ACCESS_PROTOCOL_PROFILE_FILE:-/workspace/config/access/access-protocol-profile.json}"
	"${ACCESS_AUTHORIZATION_POLICY_BUNDLE_FILE:-/workspace/config/access/access-authorization-policy-bundle.json}"
	"${ACCESS_CLIENT_TRUST_BUNDLE_FILE:-/workspace/config/access/simulation-client-trust-bundle.json}"
)

for path in "${required_config[@]}"; do
	if [[ ! -r "${path}" ]]; then
		echo "ACCESS configuration is not readable: ${path}" >&2
		exit 78
	fi
done

exec "$@"