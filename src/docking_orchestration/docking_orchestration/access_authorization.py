import json
import os

import rclpy
from docking_interfaces.msg import (
    DockingStatus,
    ReadinessEvidence,
    TransitionDecision,
    TransitionRequest,
)
from docking_orchestration.authority_client import AuthorityClient
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import Empty, String


ACTION = {
    DockingStatus.APPROACH: "enter_approach",
    DockingStatus.FINAL_APPROACH: "enter_final_approach",
    DockingStatus.SOFT_CAPTURE: "engage_soft_capture",
    DockingStatus.HARD_DOCK: "engage_hard_dock",
}

STATE_NAME = {
    DockingStatus.HOLD: "HOLD",
    DockingStatus.APPROACH: "APPROACH",
    DockingStatus.FINAL_APPROACH: "FINAL_APPROACH",
    DockingStatus.SOFT_CAPTURE: "SOFT_CAPTURE",
    DockingStatus.HARD_DOCK: "HARD_DOCK",
}

SCENARIOS = {
    "nominal": "Nominal commercial refueling",
    "expired_credential": "Expired vehicle credential",
    "corridor_violation": "Approach corridor violation",
    "latch_not_ready": "Latch telemetry incomplete",
}

DISPLAY_IDENTITY = {
    "odyssey-7": "ODYSSEY-7",
    "waystation-1": "WAYSTATION-1",
    "orbital-safety-registry": "ORBITAL SAFETY REGISTRY",
    "transition-gate": "TRANSITION GATE",
}

EVENT_SUMMARY = {
    "IDENTITY_REQUEST_VERIFIED": "Signed vehicle identity request verified",
    "SESSION_OFFER_VERIFIED": "Station challenge bound to the ACCESS session",
    "CREDENTIAL_ISSUED": "Registration and docking credentials issued",
    "CREDENTIALS_VERIFIED": "Credential signatures, validity, and holder binding verified",
    "HOLDER_PROOF_VERIFIED": "Vehicle proved control of its credential key",
    "HOLDER_PROOF_REFRESHED": "Fresh holder proof verified for this transition",
    "SESSION_AUTHORIZED": "Authenticated ACCESS session authorized",
    "AUTHORIZATION_GRANT_ISSUED": "Stage-scoped authorization grant issued",
    "AUTHORIZATION_GRANT_CONSUMED": "Single-use grant verified and consumed",
}


class AccessAuthorization(Node):
    """ROS transport adapter; all authorization decisions are made by Rust."""

    def __init__(self):
        super().__init__("access_authorization")
        self._client = AuthorityClient(
            os.environ.get("ACCESS_AUTHORITY_COMMAND", "access-authority"),
            float(os.environ.get("ACCESS_AUTHORITY_TIMEOUT_S", "5")),
        )
        self._state = DockingStatus.HOLD
        self._scenario_id = "nominal"
        self._events = []
        self._entitlements = []
        self._completed_steps = []
        self._session_id = None
        self._policy_id = None
        self._policy_version = None
        self._policy = None
        self._policy_assessments = []
        self._session_ready = False
        self._session_denial_reason = None
        self._readiness = None
        self._pending_request = None
        try:
            self._policy = self._client.request({"command": "describe"})
        except (OSError, RuntimeError, ValueError) as error:
            self.get_logger().error(f"ACCESS policy description unavailable: {error}")
        qos = QoSProfile(
            depth=1,
            reliability=ReliabilityPolicy.RELIABLE,
            durability=DurabilityPolicy.TRANSIENT_LOCAL,
        )
        self._status_publisher = self.create_publisher(
            String, "/authorization/status", qos
        )
        self._decision_publisher = self.create_publisher(
            TransitionDecision, "/docking/transition_decision", 10
        )
        self.create_subscription(
            TransitionRequest, "/docking/transition_request", self._on_request, 10
        )
        self.create_subscription(Empty, "/docking/reset", self._on_reset, 10)
        self.create_subscription(Empty, "/docking/prepare", self._on_prepare, 10)
        self.create_subscription(
            String, "/authorization/scenario", self._on_scenario, 10
        )
        self.create_subscription(String, "/docking/run", self._on_run, 10)
        self.create_subscription(
            ReadinessEvidence, "/docking/readiness", self._on_readiness, 10
        )
        self._publish_status()
        self.get_logger().info("Rust ACCESS cryptographic authority started")

    def _on_reset(self, _message):
        self._state = DockingStatus.HOLD
        self._events = []
        self._entitlements = []
        self._completed_steps = []
        self._session_id = None
        self._policy_id = None
        self._policy_version = None
        self._policy_assessments = []
        self._pending_request = None
        self._session_denial_reason = None
        try:
            session = self._client.request(
                {"command": "establish", "scenario": self._scenario_id}
            )
            self._events.extend(
                self._display_event(event, session) for event in session["events"]
            )
            self._completed_steps = [event["code"] for event in session["events"]]
            self._session_id = session["session_id"]
            self._policy_id = session["policy_id"]
            self._policy_version = session["policy_version"]
            self._session_ready = True
            self._session_denial_reason = None
            self.get_logger().info("ACCESS session cryptographically established")
        except (OSError, RuntimeError, ValueError) as error:
            self._session_ready = False
            reason = (
                "DENY_CREDENTIAL_EXPIRED"
                if "credential is not currently valid" in str(error)
                else "DENY_ACCESS_SESSION_UNAVAILABLE"
            )
            self._session_denial_reason = reason
            self._policy_assessments = [
                {
                    "rule_id": "session-establishment",
                    "action": "authorize_session",
                    "decision": "deny",
                    "reason": reason,
                    "evidence_observed_at_ms": None,
                    "rows": [
                        {
                            "control": "Credential validity",
                            "requirement": "issuer signature, holder binding, and active validity interval",
                            "observed": str(error),
                            "passed": False,
                        },
                        {
                            "control": "Session authorization",
                            "requirement": "all identity prerequisites pass",
                            "observed": "not issued",
                            "passed": False,
                        },
                    ],
                }
            ]
            self._events.append(
                {
                    "kind": "evidence",
                    "code": reason,
                    "detail": "Cryptographic session establishment failed closed",
                }
            )
            self.get_logger().warning(f"ACCESS session denied: {error}")
        self._publish_status()

    def _on_prepare(self, _message):
        self._state = DockingStatus.HOLD
        self._session_ready = False
        self._events = []
        self._entitlements = []
        self._completed_steps = []
        self._session_id = None
        self._policy_id = None
        self._policy_version = None
        self._policy_assessments = []
        self._session_denial_reason = None
        self._readiness = None
        self._publish_status()

    def _on_scenario(self, message):
        if message.data not in SCENARIOS:
            self.get_logger().warning(f"ignored unknown scenario: {message.data}")
            return
        self._scenario_id = message.data
        self._publish_status()

    def _on_run(self, message):
        self._on_scenario(message)
        self._on_reset(message)

    def _on_readiness(self, evidence):
        checks = {
            name: bool(getattr(evidence, name))
            for name in (
                "initial_hold_confirmed",
                "retreat_available",
                "relative_navigation_valid",
                "approach_corridor_clear",
                "closing_rate_within_limit",
                "alignment_within_limit",
                "capture_system_ready",
                "soft_capture_confirmed",
                "latches_ready",
                "relative_motion_stable",
            )
        }
        self._readiness = {
            "observed_at_ms": evidence.stamp.sec * 1000
            + evidence.stamp.nanosec // 1_000_000,
            "range_m": evidence.range_m,
            "closing_rate_mps": evidence.closing_rate_mps,
            "checks": checks,
        }
        if (
            self._session_ready
            and checks["initial_hold_confirmed"]
            and "INITIAL_HOLD_CONFIRMED" not in self._completed_steps
        ):
            self._completed_steps.append("INITIAL_HOLD_CONFIRMED")
            self._events.append(
                {
                    "kind": "readiness",
                    "code": "INITIAL_HOLD_CONFIRMED",
                    "detail": "Fresh station-local hold and retreat evidence received",
                }
            )
            self._publish_status()
        if self._pending_request is not None:
            request = self._pending_request
            self._pending_request = None
            self._evaluate_request(request)

    def _on_request(self, request):
        if self._pending_request is not None:
            self.get_logger().warning("ignored duplicate pending transition request")
            return
        self._pending_request = request

    def _evaluate_request(self, request):
        previous = self._state
        try:
            if not self._session_ready:
                raise RuntimeError("ACCESS session is not authorized")
            if self._readiness is None:
                raise RuntimeError("station-local readiness evidence is unavailable")
            outcome = self._client.request(
                {
                    "command": "transition",
                    "requested_state": request.requested_state,
                    "readiness": self._readiness,
                }
            )
            approved = outcome["approved"]
            reason = outcome["reason"]
            self._state = outcome["resulting_state"]
            self._events.extend(
                self._display_event(event, outcome) for event in outcome["events"]
            )
            self._record_policy_assessment(outcome)
            if approved:
                action = ACTION[request.requested_state]
                self._entitlements.append(
                    {
                        "id": outcome["grant_id"],
                        "action": action,
                        "stage": STATE_NAME[self._state],
                        "ttl_s": outcome["entitlement_ttl_s"],
                        "status": "consumed",
                        "rule_id": outcome["rule_id"],
                    }
                )
        except (OSError, RuntimeError, ValueError, KeyError) as error:
            approved = False
            reason = self._session_denial_reason or "DENY_ACCESS_AUTHORITY_ERROR"
            if self._session_denial_reason is None:
                self._session_ready = False
                self._session_denial_reason = reason
                self._events.append(
                    {
                        "kind": "evidence",
                        "code": reason,
                        "detail": "Authorization authority failed closed",
                    }
                )
                self.get_logger().error(f"ACCESS transition failed: {error}")

        decision = TransitionDecision()
        decision.approved = approved
        decision.previous_state = previous
        decision.resulting_state = self._state
        decision.authority = "rust_access_authority"
        decision.reason = reason
        self._decision_publisher.publish(decision)
        self._publish_status()

    def _record_policy_assessment(self, outcome):
        rule_id = outcome.get("rule_id")
        rules = self._policy.get("stage_policies", []) if self._policy else []
        rule = next((item for item in rules if item["rule_id"] == rule_id), None)
        if rule is None:
            return
        reason = outcome["reason"]
        rows = [
            {
                "control": "Credential profiles",
                "requirement": ", ".join(rule["required_credential_profiles"]),
                "observed": "verified and holder-bound",
                "passed": reason != "DENY_CREDENTIAL_REQUIRED",
            },
            {
                "control": "Holder proof",
                "requirement": f"fresh within {rule.get('maximum_proof_age_s', 0)}s",
                "observed": "challenge response refreshed for this request",
                "passed": reason != "DENY_HOLDER_PROOF",
            },
        ]
        for check in rule["readiness"]["required_checks"]:
            passed = bool(self._readiness["checks"].get(check, False))
            rows.append(
                {
                    "control": check.replace("_", " ").title(),
                    "requirement": f"station-local evidence <= {rule['readiness']['maximum_age_ms']}ms old",
                    "observed": "pass" if passed else "failed",
                    "passed": passed and reason != "DENY_READINESS_STALE",
                }
            )
        constraints = rule.get("constraints", {})
        if constraints.get("max_range_m") is not None:
            limit = constraints["max_range_m"]
            observed = self._readiness["range_m"]
            rows.append(
                {
                    "control": "Range constraint",
                    "requirement": f"range <= {limit:.3f}m",
                    "observed": f"{observed:.3f}m",
                    "passed": observed <= limit,
                }
            )
        if constraints.get("max_closing_rate_mps") is not None:
            limit = constraints["max_closing_rate_mps"]
            observed = self._readiness["closing_rate_mps"]
            rows.append(
                {
                    "control": "Closing-rate constraint",
                    "requirement": f"rate <= {limit:.3f}m/s",
                    "observed": f"{observed:.3f}m/s",
                    "passed": observed <= limit,
                }
            )
        rows.append(
            {
                "control": "Stage entitlement",
                "requirement": f"single use; TTL {rule['entitlement_ttl_s']}s",
                "observed": (
                    f"{outcome['grant_id']} consumed"
                    if outcome["approved"]
                    else f"not issued: {reason}"
                ),
                "passed": outcome["approved"],
            }
        )
        self._policy_assessments.append(
            {
                "rule_id": rule_id,
                "action": rule["action"],
                "from_stage": rule["from_stage"],
                "to_stage": rule["to_stage"],
                "decision": "allow" if outcome["approved"] else "deny",
                "reason": reason,
                "evidence_observed_at_ms": self._readiness["observed_at_ms"],
                "rows": rows,
            }
        )
        self._policy_assessments = self._policy_assessments[-6:]

    def _display_event(self, event, context=None):
        value = dict(event)
        value["kind"] = "message" if event.get("message_type") else "evidence"
        value["summary"] = EVENT_SUMMARY.get(event["code"], event["detail"])
        if context:
            for name in (
                "session_id",
                "policy_id",
                "policy_version",
                "rule_id",
                "grant_id",
                "entitlement_ttl_s",
            ):
                if context.get(name) is not None:
                    value[name] = context[name]
        if event.get("from"):
            value["from"] = DISPLAY_IDENTITY.get(event["from"], event["from"].upper())
        if event.get("to"):
            value["to"] = DISPLAY_IDENTITY.get(event["to"], event["to"].upper())
        return value

    def _publish_status(self):
        trust_bundle = (self._policy or {}).get("trust_bundle") or {}
        payload = {
            "mode": "LIVE ACCESS PROTOCOL",
            "scenario_id": self._scenario_id,
            "scenario": SCENARIOS[self._scenario_id],
            "station": "Waystation-1 / port-3",
            "chaser": "Odyssey-7",
            "operator": "Lunar Logistics",
            "session_id": self._session_id or "pending",
            "policy_id": self._policy_id
            or (self._policy["policy_id"] if self._policy else "pending"),
            "policy_version": self._policy_version
            or (self._policy["policy_version"] if self._policy else None),
            "trust_bundle": (
                f"{trust_bundle.get('bundle_id', 'unspecified')}@"
                f"{trust_bundle.get('minimum_version', '-')}"
                if trust_bundle
                else "unavailable"
            ),
            "policy": self._policy,
            "policy_assessments": self._policy_assessments,
            "phase": "SESSION_AUTHORIZED" if self._session_ready else "IDLE",
            "completed_steps": self._completed_steps,
            "events": self._events[-32:],
            "entitlements": self._entitlements[-6:],
        }
        message = String()
        message.data = json.dumps(payload, separators=(",", ":"))
        self._status_publisher.publish(message)

    def destroy_node(self):
        self._client.close()
        return super().destroy_node()


def main(args=None):
    rclpy.init(args=args)
    node = AccessAuthorization()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()