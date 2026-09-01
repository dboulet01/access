import json
import os
import uuid

import rclpy
from docking_interfaces.msg import DockingStatus, ReadinessEvidence, TransitionDecision
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

EVENT_SUMMARY = {
    "ACCESS_REQUEST_VERIFIED": "Signed ACCESS request verified",
    "SESSION_OFFER_VERIFIED": "Station challenge bound to the ACCESS session",
    "CREDENTIAL_ISSUED": "Registration and docking credentials issued",
    "CREDENTIALS_VERIFIED": "Credential signatures, validity, and holder binding verified",
    "HOLDER_PROOF_VERIFIED": "Vehicle proved control of its credential key",
    "HOLDER_PROOF_REFRESHED": "Fresh holder proof verified for this transition",
    "ACCESS_INITIAL_CLAIMS_ALLOWED": "ACCESS policy authorized the verified initial claims",
    "ACCESS_STAGE_POLICY_ALLOWED": "ACCESS policy authorized the requested use-case transition",
    "SESSION_AUTHORIZED": "Authenticated ACCESS session authorized",
    "AUTHORIZATION_GRANT_ISSUED": "Stage-scoped authorization grant issued",
    "AUTHORIZATION_GRANT_CONSUMED": "Single-use grant verified and consumed",
}


class StationAccess(Node):
    """Station-side ACCESS participant that owns policy evaluation and gate decisions."""

    def __init__(self):
        super().__init__("station_access")
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
        self._protocol_profile_id = None
        self._protocol_profile_version = None
        self._protocol_profile = None
        self._authorization_assessments = []
        self._session_ready = False
        self._session_denial_reason = None
        self._readiness = None
        self._event_sequence = 0

        try:
            self._protocol_profile = self._client.request({"command": "describe"})
        except (OSError, RuntimeError, ValueError) as error:
            self.get_logger().error(f"ACCESS policy description unavailable: {error}")

        qos = QoSProfile(
            depth=1,
            reliability=ReliabilityPolicy.RELIABLE,
            durability=DurabilityPolicy.TRANSIENT_LOCAL,
        )
        self._status_publisher = self.create_publisher(String, "/authorization/status", qos)
        self._decision_publisher = self.create_publisher(
            TransitionDecision, "/docking/transition_decision", 10
        )
        self._station_protocol_publisher = self.create_publisher(
            String, "/access/station_to_chaser", 10
        )

        self.create_subscription(
            String, "/access/chaser_to_station", self._on_chaser_protocol, 10
        )
        self.create_subscription(Empty, "/docking/prepare", self._on_prepare, 10)
        self.create_subscription(Empty, "/docking/reset", self._on_reset, 10)
        self.create_subscription(String, "/docking/run", self._on_run, 10)
        self.create_subscription(String, "/authorization/scenario", self._on_scenario, 10)
        self.create_subscription(
            ReadinessEvidence, "/docking/readiness", self._on_readiness, 10
        )
        self._publish_status()
        self.get_logger().info(
            "station ACCESS node ready; secure transport assumed by mission comms layer"
        )

    def _on_prepare(self, _message):
        self._clear_runtime_state()
        self._publish_status()

    def _on_reset(self, _message):
        self._clear_runtime_state()
        self._publish_status()

    def _on_run(self, message):
        if message.data in SCENARIOS:
            self._scenario_id = message.data
        self._clear_runtime_state()
        self._publish_status()

    def _on_scenario(self, message):
        if message.data not in SCENARIOS:
            self.get_logger().warning(f"ignored unknown scenario: {message.data}")
            return
        self._scenario_id = message.data
        self._publish_status()

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
            self._append_event(
                {
                    "kind": "readiness",
                    "code": "INITIAL_HOLD_CONFIRMED",
                    "detail": "Fresh station-local hold and retreat evidence received",
                    "summary": "Initial hold validated by station-local readiness monitor",
                }
            )
            self._publish_status()

    def _on_chaser_protocol(self, message):
        try:
            payload = json.loads(message.data)
        except (json.JSONDecodeError, TypeError):
            self.get_logger().warning("ignored malformed chaser protocol message")
            return
        self._record_protocol_event(
            code="CHASER_PROTOCOL_MESSAGE",
            summary="Station received chaser ACCESS protocol message",
            from_id="ODYSSEY-7",
            to_id="WAYSTATION-1",
            message_type=payload.get("kind", "UNKNOWN"),
            payload=payload,
            direction="inbound",
        )
        kind = payload.get("kind")
        if kind == "access_request":
            self._handle_access_request(payload)
            return
        if kind == "transition_request":
            self._handle_transition_request(payload)
            return
        if kind == "entitlement_presentation":
            self._handle_entitlement_presentation(payload)
            return
        self.get_logger().warning(f"ignored unknown protocol message kind: {kind}")

    def _handle_access_request(self, payload):
        if payload.get("scenario_id") in SCENARIOS:
            self._scenario_id = payload["scenario_id"]

        try:
            self._append_event(
                {
                    "kind": "evidence",
                    "code": "AUTHORITY_REQUEST_ESTABLISH",
                    "detail": f"scenario={self._scenario_id}",
                    "summary": "Station requested session establishment from Rust authority",
                    "authority_request": {
                        "command": "establish",
                        "scenario": self._scenario_id,
                    },
                }
            )
            session = self._client.request({"command": "establish", "scenario": self._scenario_id})
            self._append_event(
                {
                    "kind": "evidence",
                    "code": "AUTHORITY_RESPONSE_ESTABLISH",
                    "detail": "authority returned authorized session context",
                    "summary": "Rust authority established ACCESS session",
                    "authority_response": {
                        "session_id": session.get("session_id"),
                        "protocol_profile_id": session.get("protocol_profile_id"),
                        "protocol_profile_version": session.get("protocol_profile_version"),
                        "event_count": len(session.get("events", [])),
                    },
                }
            )
            for event in session["events"]:
                self._append_event(self._display_event(event, session))
            self._completed_steps = [event["code"] for event in session["events"]]
            self._session_id = session["session_id"]
            self._protocol_profile_id = session["protocol_profile_id"]
            self._protocol_profile_version = session["protocol_profile_version"]
            self._session_ready = True
            self._session_denial_reason = None

            self._publish_station_protocol(
                {
                    "protocol_version": "1.0",
                    "kind": "session_authorized",
                    "message_id": f"msg-{uuid.uuid4().hex}",
                    "from": "WAYSTATION-1",
                    "to": "ODYSSEY-7",
                    "session_id": self._session_id,
                    "protocol_profile_id": self._protocol_profile_id,
                    "protocol_profile_version": self._protocol_profile_version,
                    "secure_transport_assumed": True,
                },
                code="SESSION_AUTHORIZED_OUTBOUND",
                summary="Waystation-1 authorized ACCESS session and returned session context",
            )
            self.get_logger().info(
                f"ACCESS session established across nodes: session_id={self._session_id}"
            )
        except (OSError, RuntimeError, ValueError) as error:
            self._session_ready = False
            reason = (
                "DENY_CREDENTIAL_EXPIRED"
                if "credential is not currently valid" in str(error)
                else "DENY_ACCESS_SESSION_UNAVAILABLE"
            )
            self._session_denial_reason = reason
            self._authorization_assessments = [
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
            self._append_event(
                {
                    "kind": "evidence",
                    "code": reason,
                    "detail": "Cryptographic session establishment failed closed",
                    "summary": "Session denied by station authority",
                    "authority_error": str(error),
                }
            )
            self._append_event(
                {
                    "kind": "evidence",
                    "code": "AUTHORITY_RESPONSE_ESTABLISH_ERROR",
                    "detail": "authority failed or denied session establishment",
                    "summary": "Rust authority returned establish failure",
                    "authority_error": str(error),
                }
            )
            self._publish_station_protocol(
                {
                    "protocol_version": "1.0",
                    "kind": "session_denied",
                    "message_id": f"msg-{uuid.uuid4().hex}",
                    "from": "WAYSTATION-1",
                    "to": "ODYSSEY-7",
                    "reason": reason,
                    "secure_transport_assumed": True,
                },
                code="SESSION_DENIED_OUTBOUND",
                summary="Waystation-1 denied ACCESS session establishment",
            )
            self.get_logger().warning(f"ACCESS session denied: {error}")

        self._publish_status()

    def _handle_transition_request(self, payload):
        requested_state = int(payload.get("requested_state", -1))
        requester = payload.get("requester", "chaser_access")
        request_reason = payload.get("reason", "no reason provided")

        self._append_event(
            {
                "kind": "message",
                "code": "TRANSITION_REQUESTED",
                "detail": f"requested_state={requested_state}; requester={requester}; basis={request_reason}",
                "summary": "Odyssey-7 requested protected stage transition",
                "from": "ODYSSEY-7",
                "to": "WAYSTATION-1",
                "message_type": "TRANSITION_REQUEST",
                "session_id": self._session_id,
            }
        )

        previous = self._state
        outcome = {}
        try:
            if not self._session_ready:
                raise RuntimeError("ACCESS session is not authorized")
            if self._readiness is None:
                raise RuntimeError("station-local readiness evidence is unavailable")
            self._append_event(
                {
                    "kind": "evidence",
                    "code": "AUTHORITY_REQUEST_TRANSITION",
                    "detail": f"requested_state={requested_state}",
                    "summary": "Station requested transition authorization from Rust authority",
                    "authority_request": {
                        "command": "transition",
                        "requested_state": requested_state,
                        "readiness": self._readiness,
                    },
                }
            )
            outcome = self._client.request(
                {
                    "command": "transition",
                    "requested_state": requested_state,
                    "readiness": self._readiness,
                }
            )
            approved = outcome["approved"]
            reason = outcome["reason"]
            self._append_event(
                {
                    "kind": "evidence",
                    "code": "AUTHORITY_RESPONSE_TRANSITION",
                    "detail": f"approved={approved}; reason={reason}",
                    "summary": "Rust authority returned transition policy decision",
                    "authority_response": outcome,
                }
            )
            for event in outcome["events"]:
                self._append_event(self._display_event(event, outcome))
            self._record_authorization_assessment(outcome)
            if approved:
                action = ACTION[requested_state]
                self._entitlements.append(
                    {
                        "id": outcome["grant_id"],
                        "action": action,
                        "stage": STATE_NAME[requested_state],
                        "ttl_s": outcome["entitlement_ttl_s"],
                        "status": "issued",
                        "rule_id": outcome["rule_id"],
                    }
                )
                self._publish_station_protocol(
                    {
                        "protocol_version": "1.0",
                        "kind": "authorization_grant",
                        "message_id": f"msg-{uuid.uuid4().hex}",
                        "from": "WAYSTATION-1",
                        "to": "ODYSSEY-7",
                        "session_id": self._session_id,
                        "approved": True,
                        "reason": reason,
                        "requested_state": requested_state,
                        "authority_id": "waystation-1",
                        "grant_id": outcome["grant_id"],
                        "grant_expires_at_s": outcome["grant_expires_at_s"],
                        "signed_grant_hex": outcome["signed_grant_hex"],
                        "secure_transport_assumed": True,
                    },
                    code="AUTHORIZATION_GRANT_OUTBOUND",
                    summary="Waystation-1 issued entitlement to Odyssey-7",
                )
                self._publish_status()
                return
        except (OSError, RuntimeError, ValueError, KeyError) as error:
            approved = False
            reason = self._session_denial_reason or "DENY_ACCESS_AUTHORITY_ERROR"
            if self._session_denial_reason is None:
                self._session_ready = False
                self._session_denial_reason = reason
                self._append_event(
                    {
                        "kind": "evidence",
                        "code": reason,
                        "detail": "Authorization authority failed closed",
                        "summary": "Station authority failed closed",
                        "authority_error": str(error),
                    }
                )
                self._append_event(
                    {
                        "kind": "evidence",
                        "code": "AUTHORITY_RESPONSE_TRANSITION_ERROR",
                        "detail": "authority failed before transition decision",
                        "summary": "Rust authority transition request failed",
                        "authority_error": str(error),
                    }
                )
                self.get_logger().error(f"ACCESS transition failed: {error}")

        decision = TransitionDecision()
        decision.approved = approved
        decision.previous_state = previous
        decision.resulting_state = self._state
        decision.authority = "waystation-1"
        decision.reason = reason
        decision.grant_id = outcome.get("grant_id") or ""
        decision.grant_expires_at_s = outcome.get("grant_expires_at_s") or 0
        decision.signed_grant_hex = outcome.get("signed_grant_hex") or ""
        authorization_decision = outcome.get("authorization_decision") or {}
        authorization_policy = authorization_decision.get("policy") or {}
        decision.authorization_policy_bundle_id = authorization_policy.get("bundle_id", "")
        decision.authorization_policy_bundle_version = authorization_policy.get("bundle_version", 0)
        decision.authorization_policy_sha256 = authorization_policy.get("policy_sha256", "")
        self._decision_publisher.publish(decision)
        self._append_event(
            {
                "kind": "evidence",
                "code": "TRANSITION_DECISION_PUBLISHED",
                "detail": f"published decision approved={approved}; previous={previous}; resulting={self._state}",
                "summary": "Station published protected transition decision to controller",
                "decision": {
                    "approved": approved,
                    "previous_state": previous,
                    "resulting_state": self._state,
                    "reason": reason,
                },
            }
        )

        self._append_event(
            {
                "kind": "message",
                "code": reason,
                "detail": f"approved={approved}; resulting_state={self._state}; reason={reason}",
                "summary": "Waystation-1 returned transition decision to Odyssey-7",
                "from": "WAYSTATION-1",
                "to": "ODYSSEY-7",
                "message_type": "TRANSITION_DECISION",
                "session_id": self._session_id,
            }
        )

        self._publish_station_protocol(
            {
                "protocol_version": "1.0",
                "kind": "transition_decision",
                "message_id": f"msg-{uuid.uuid4().hex}",
                "from": "WAYSTATION-1",
                "to": "ODYSSEY-7",
                "session_id": self._session_id,
                "approved": approved,
                "reason": reason,
                "resulting_state": int(self._state),
                "authority_id": "waystation-1",
                "grant_id": outcome.get("grant_id"),
                "grant_expires_at_s": outcome.get("grant_expires_at_s"),
                "signed_grant_hex": outcome.get("signed_grant_hex"),
                "authorization_decision": outcome.get("authorization_decision"),
                "secure_transport_assumed": True,
            },
            code="TRANSITION_DECISION_OUTBOUND",
            summary="Waystation-1 returned signed transition decision context",
        )
        self._publish_status()

    def _handle_entitlement_presentation(self, payload):
        requested_state = int(payload.get("requested_state", -1))
        previous = self._state
        outcome = {}
        try:
            if self._readiness is None:
                raise RuntimeError("station-local readiness evidence is unavailable")
            outcome = self._client.request(
                {
                    "command": "redeem_entitlement",
                    "requested_state": requested_state,
                    "entitlement_hex": payload["signed_grant_hex"],
                    "readiness": self._readiness,
                }
            )
            approved = outcome["approved"]
            reason = outcome["reason"]
            self._state = outcome["resulting_state"]
            for entitlement in self._entitlements:
                if entitlement["id"] == outcome.get("grant_id"):
                    entitlement["status"] = "consumed"
                    break
            for event in outcome["events"]:
                self._append_event(self._display_event(event, outcome))
        except (OSError, RuntimeError, ValueError, KeyError) as error:
            approved = False
            reason = "DENY_ENTITLEMENT_REDEMPTION"
            self.get_logger().error(f"ACCESS entitlement redemption failed: {error}")

        decision = TransitionDecision()
        decision.approved = approved
        decision.previous_state = previous
        decision.resulting_state = self._state
        decision.authority = "waystation-1"
        decision.reason = reason
        decision.grant_id = outcome.get("grant_id") or payload.get("grant_id", "")
        decision.grant_expires_at_s = outcome.get("grant_expires_at_s") or 0
        decision.signed_grant_hex = outcome.get("signed_grant_hex") or ""
        authorization_decision = outcome.get("authorization_decision") or {}
        authorization_policy = authorization_decision.get("policy") or {}
        decision.authorization_policy_bundle_id = authorization_policy.get("bundle_id", "")
        decision.authorization_policy_bundle_version = authorization_policy.get("bundle_version", 0)
        decision.authorization_policy_sha256 = authorization_policy.get("policy_sha256", "")
        self._decision_publisher.publish(decision)
        self._publish_station_protocol(
            {
                "protocol_version": "1.0",
                "kind": "transition_decision",
                "message_id": f"msg-{uuid.uuid4().hex}",
                "from": "WAYSTATION-1",
                "to": "ODYSSEY-7",
                "session_id": self._session_id,
                "approved": approved,
                "reason": reason,
                "resulting_state": int(self._state),
                "authority_id": "waystation-1",
                "grant_id": decision.grant_id,
                "secure_transport_assumed": True,
            },
            code="TRANSITION_DECISION_OUTBOUND",
            summary="Waystation-1 returned enforced transition outcome",
        )
        self._publish_status()

    def _record_authorization_assessment(self, outcome):
        rule_id = outcome.get("rule_id")
        rules = self._protocol_profile.get("stage_rules", []) if self._protocol_profile else []
        rule = next((item for item in rules if item["rule_id"] == rule_id), None)
        if rule is None:
            return
        reason = outcome["reason"]
        authorization_decision = outcome.get("authorization_decision") or {}
        authorization_policy = authorization_decision.get("policy") or {}
        evidence_age_ms = self._now_ms() - self._readiness["observed_at_ms"]
        maximum_age_ms = rule["readiness"]["maximum_age_ms"]
        rows = [
            {
                "control": "Protocol profile",
                "requirement": f"{outcome.get('protocol_profile_id')} v{outcome.get('protocol_profile_version')}",
                "observed": f"rule {rule_id}",
                "passed": not reason.startswith("DENY_POLICY"),
            },
            {
                "control": "Holder proof",
                "requirement": f"fresh within {rule.get('maximum_proof_age_s', 0)}s",
                "observed": "challenge response refreshed for this request",
                "passed": reason != "DENY_HOLDER_PROOF",
            },
            {
                "control": "Evidence freshness",
                "requirement": f"age <= {maximum_age_ms}ms",
                "observed": f"{evidence_age_ms}ms",
                "passed": evidence_age_ms <= maximum_age_ms,
            },
            {
                "control": "ACCESS authorization policy",
                "requirement": f"{authorization_policy.get('bundle_id', 'active bundle')} permits {rule['action']}",
                "observed": authorization_decision.get("decision", "deny"),
                "passed": authorization_decision.get("decision") == "allow",
            },
            {
                "control": "Stage entitlement",
                "requirement": f"single use; TTL {rule['entitlement_ttl_s']}s",
                "observed": (
                    f"{outcome['grant_id']} consumed"
                    if outcome["approved"]
                    else f"not issued: {reason}"
                ),
                "passed": outcome["approved"],
            },
        ]
        self._authorization_assessments.append(
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
        self._authorization_assessments = self._authorization_assessments[-6:]

    def _display_event(self, event, context=None):
        value = dict(event)
        value["kind"] = "message" if event.get("message_type") else "evidence"
        value["summary"] = EVENT_SUMMARY.get(event["code"], event["detail"])
        if context:
            for name in (
                "session_id",
                "protocol_profile_id",
                "protocol_profile_version",
                "rule_id",
                "grant_id",
                "entitlement_ttl_s",
            ):
                if context.get(name) is not None:
                    value[name] = context[name]
        if event.get("from"):
            value["from"] = event["from"].upper()
        if event.get("to"):
            value["to"] = event["to"].upper()
        return value

    def _shape_fields(self, payload):
        return sorted(payload.keys())

    def _record_protocol_event(
        self,
        code,
        summary,
        from_id,
        to_id,
        message_type,
        payload,
        direction,
    ):
        fields = self._shape_fields(payload)
        self._append_event(
            {
                "kind": "message",
                "code": code,
                "detail": f"{direction}; fields={','.join(fields)}",
                "summary": summary,
                "from": from_id,
                "to": to_id,
                "message_type": message_type,
                "session_id": payload.get("session_id", self._session_id),
                "protocol_version": payload.get("protocol_version", "1.0"),
                "protocol_shape": fields,
                "protocol_payload": payload,
            }
        )

    def _now_ms(self):
        now = self.get_clock().now().to_msg()
        return now.sec * 1000 + now.nanosec // 1_000_000

    def _append_event(self, event):
        value = dict(event)
        self._event_sequence += 1
        value.setdefault("observed_at_ms", self._now_ms())
        value["event_sequence"] = self._event_sequence
        self._events.append(value)
        self._events = self._events[-256:]

    def _publish_station_protocol(self, payload, code, summary):
        self._record_protocol_event(
            code=code,
            summary=summary,
            from_id="WAYSTATION-1",
            to_id="ODYSSEY-7",
            message_type=payload.get("kind", "UNKNOWN"),
            payload=payload,
            direction="outbound",
        )
        message = String()
        message.data = json.dumps(payload, separators=(",", ":"))
        self._station_protocol_publisher.publish(message)

    def _publish_status(self):
        trust_bundle = (self._protocol_profile or {}).get("trust_bundle") or {}
        payload = {
            "mode": "LIVE ACCESS PROTOCOL",
            "scenario_id": self._scenario_id,
            "scenario": SCENARIOS[self._scenario_id],
            "station": "Waystation-1 / port-3",
            "chaser": "Odyssey-7",
            "operator": "Lunar Logistics",
            "session_id": self._session_id or "pending",
            "protocol_profile_id": self._protocol_profile_id
            or (self._protocol_profile["profile_id"] if self._protocol_profile else "pending"),
            "protocol_profile_version": self._protocol_profile_version
            or (self._protocol_profile["profile_version"] if self._protocol_profile else None),
            "trust_bundle": (
                f"{trust_bundle.get('bundle_id', 'unspecified')}@"
                f"{trust_bundle.get('minimum_version', '-')}"
                if trust_bundle
                else "unavailable"
            ),
            "protocol_profile": self._protocol_profile,
            "authorization_assessments": self._authorization_assessments,
            "phase": "SESSION_AUTHORIZED" if self._session_ready else "IDLE",
            "completed_steps": self._completed_steps,
            "events": self._events,
            "entitlements": self._entitlements[-6:],
            "transport_profile": {
                "assumption": "secure-channel-pre-established",
                "access_scope": "application authorization over established comms",
            },
        }
        message = String()
        message.data = json.dumps(payload, separators=(",", ":"))
        self._status_publisher.publish(message)

    def _clear_runtime_state(self):
        self._state = DockingStatus.HOLD
        self._events = []
        self._entitlements = []
        self._completed_steps = []
        self._session_id = None
        self._protocol_profile_id = None
        self._protocol_profile_version = None
        self._authorization_assessments = []
        self._session_ready = False
        self._session_denial_reason = None
        self._readiness = None
        self._event_sequence = 0

    def destroy_node(self):
        self._client.close()
        return super().destroy_node()


def main(args=None):
    rclpy.init(args=args)
    node = StationAccess()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()
