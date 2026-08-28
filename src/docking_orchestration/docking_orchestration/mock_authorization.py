import json

import rclpy
from docking_interfaces.msg import DockingStatus, TransitionDecision, TransitionRequest
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import Empty, String


STATE_NAME = {
    DockingStatus.HOLD: "HOLD",
    DockingStatus.APPROACH: "APPROACH",
    DockingStatus.FINAL_APPROACH: "FINAL_APPROACH",
    DockingStatus.SOFT_CAPTURE: "SOFT_CAPTURE",
    DockingStatus.HARD_DOCK: "HARD_DOCK",
}

ACTION = {
    DockingStatus.APPROACH: "enter_approach",
    DockingStatus.FINAL_APPROACH: "enter_final_approach",
    DockingStatus.SOFT_CAPTURE: "engage_soft_capture",
    DockingStatus.HARD_DOCK: "engage_hard_dock",
}

STAGE_EVIDENCE = {
    DockingStatus.APPROACH: "registration, holder proof, session, initial hold",
    DockingStatus.FINAL_APPROACH: "identity, IDSS compatibility, corridor, closing rate",
    DockingStatus.SOFT_CAPTURE: "interface compatibility, alignment, capture readiness",
    DockingStatus.HARD_DOCK: "soft capture, latch readiness, relative-motion stability",
}

ACTION_SUMMARY = {
    "enter_approach": {
        "request": "Odyssey-7 is asking to leave hold and enter the approach corridor.",
        "allow": "Waystation-1 approved entry into the approach corridor.",
    },
    "enter_final_approach": {
        "request": "Odyssey-7 is asking to begin final approach to port 3.",
        "allow": "Waystation-1 confirmed the corridor is safe for final approach.",
    },
    "engage_soft_capture": {
        "request": "Odyssey-7 is asking the station to engage soft capture.",
        "allow": "Waystation-1 confirmed alignment and approved soft capture.",
    },
    "engage_hard_dock": {
        "request": "Odyssey-7 is asking to lock the docking interface.",
        "allow": "Waystation-1 confirmed latch readiness and approved hard dock.",
    },
}

ONBOARDING_STEPS = [
    (0.0, "TRUST_BUNDLE_LOADED", "bundle=waystation-1-trust@42; digest=sha256:9f2c...71ae; max_age=7d"),
    (1.5, "ISSUER_STAGED", "issuers=Orbital Safety Registry,Lunar Logistics; profiles=vehicle-registration,idss-interface"),
    (3.5, "CHALLENGE_ISSUED", "nonce=station-972; alg=EdDSA; audience=waystation-1/port-3; expires_in=30s"),
    (6.5, "CREDENTIALS_VERIFIED", "COSE_Sign1 verified; issuer=did:web:lunar-logistics.example; profile=IDSS-2024; status=active"),
    (9.5, "HOLDER_PROOF_VERIFIED", "did:peer holder proof valid; challenge=station-972; key=Ed25519#encounter-7; age=184ms"),
    (12.5, "SESSION_AUTHORIZED", "session=dock-2026-1842; port=port-3; scope=refuel:methane,dock; expires_in=300s"),
    (15.5, "INITIAL_HOLD_CONFIRMED", "range=3.320m; relative_rate=0.000m/s; attitude_error=0.08deg; corridor=clear"),
]

MESSAGE_RESPONSE_DELAY_S = 2.5

SCENARIOS = {
    "nominal": {
        "name": "Nominal commercial refueling",
        "deny_action": None,
    },
    "expired_credential": {
        "name": "Expired vehicle credential",
        "deny_action": "enter_approach",
        "reason": "DENY_CREDENTIAL_EXPIRED",
        "detail": "vehicle_registration exp=2026-08-26T22:00Z; verifier_time=2026-08-27T14:32Z; allowed_skew=120s",
        "summary": "Waystation-1 denied approach because Odyssey-7's registration credential has expired.",
    },
    "corridor_violation": {
        "name": "Approach corridor violation",
        "deny_action": "enter_final_approach",
        "reason": "DENY_CORRIDOR_CONSTRAINT",
        "detail": "cross_track_error=0.42m exceeds 0.20m; closing_rate=0.18m/s; corridor=port-3-RBAR",
        "summary": "Waystation-1 denied final approach because Odyssey-7 is outside the approved corridor.",
    },
    "latch_not_ready": {
        "name": "Latch telemetry incomplete",
        "deny_action": "engage_hard_dock",
        "reason": "DENY_LATCH_NOT_READY",
        "detail": "ready_latches=10/12; ring_load=3.8kN; relative_rate=0.008m/s; require=12/12",
        "summary": "Waystation-1 denied hard dock because two latches are not reporting ready.",
    },
}

STEP_EVENT = {
    "TRUST_BUNDLE_LOADED": {"kind": "configuration", "scope": "STATION LOCAL"},
    "ISSUER_STAGED": {"kind": "configuration", "scope": "STATION LOCAL"},
    "CHALLENGE_ISSUED": {
        "kind": "message",
        "from": "WAYSTATION-1",
        "to": "ODYSSEY-7",
        "message_type": "IDENTITY_CHALLENGE",
        "summary": "Waystation-1 is asking Odyssey-7 to prove its identity for this docking session.",
    },
    "CREDENTIALS_VERIFIED": {
        "kind": "message",
        "from": "ODYSSEY-7",
        "to": "WAYSTATION-1",
        "message_type": "CREDENTIAL_PRESENTATION",
        "summary": "Odyssey-7 sent its registration and docking compatibility credentials.",
    },
    "HOLDER_PROOF_VERIFIED": {
        "kind": "message",
        "from": "ODYSSEY-7",
        "to": "WAYSTATION-1",
        "message_type": "HOLDER_PROOF",
        "summary": "Odyssey-7 proved it controls the identity used for this encounter.",
    },
    "SESSION_AUTHORIZED": {
        "kind": "message",
        "from": "WAYSTATION-1",
        "to": "ODYSSEY-7",
        "message_type": "SESSION_AUTHORIZATION",
        "summary": "Waystation-1 authorized Odyssey-7 to use port 3 for this session.",
    },
    "INITIAL_HOLD_CONFIRMED": {"kind": "readiness", "scope": "STATION LOCAL"},
}


class MockAuthorization(Node):
    """Policy-shaped demo authority; cryptographic verification is intentionally mocked."""

    def __init__(self):
        super().__init__("mock_authorization")
        self._state = DockingStatus.HOLD
        self._started_at = self.get_clock().now()
        self._published_steps = 0
        self._events = []
        self._entitlements = []
        self._decision_sequence = 0
        self._pending_request = None
        self._decision_due_at = None
        self._scenario_id = "nominal"
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
        self.create_subscription(
            String, "/authorization/scenario", self._on_scenario, 10
        )
        self.create_timer(0.1, self._advance_onboarding)
        self.get_logger().info("mock policy authority started for commercial refueling demo")

    def _advance_onboarding(self):
        elapsed = (self.get_clock().now() - self._started_at).nanoseconds / 1e9
        while (
            self._published_steps < len(ONBOARDING_STEPS)
            and elapsed >= ONBOARDING_STEPS[self._published_steps][0]
        ):
            _, code, detail = ONBOARDING_STEPS[self._published_steps]
            self._events.append(
                {"code": code, "detail": detail, **STEP_EVENT[code]}
            )
            self._published_steps += 1
            self._publish_status()
        self._process_pending_request()

    def _on_request(self, request):
        if self._pending_request is not None:
            return
        action = ACTION.get(request.requested_state, "unknown")
        self._events.append(
            {
                "kind": "message",
                "code": "TRANSITION_REQUESTED",
                "detail": (
                    f"action={action}; state={STATE_NAME.get(self._state)}; "
                    f"requester={request.requester}; basis={request.reason}"
                ),
                "from": "ODYSSEY-7",
                "to": "WAYSTATION-1",
                "message_type": "TRANSITION_REQUEST",
                "summary": ACTION_SUMMARY[action]["request"],
            }
        )
        self._pending_request = request
        self._decision_due_at = self.get_clock().now() + rclpy.duration.Duration(
            seconds=MESSAGE_RESPONSE_DELAY_S
        )
        self._publish_status()
        self.get_logger().info(f"{action}: evaluating request")

    def _process_pending_request(self):
        if (
            self._pending_request is None
            or self.get_clock().now() < self._decision_due_at
        ):
            return
        request = self._pending_request
        self._pending_request = None
        self._decision_due_at = None
        expected = {
            DockingStatus.HOLD: DockingStatus.APPROACH,
            DockingStatus.APPROACH: DockingStatus.FINAL_APPROACH,
            DockingStatus.FINAL_APPROACH: DockingStatus.SOFT_CAPTURE,
            DockingStatus.SOFT_CAPTURE: DockingStatus.HARD_DOCK,
        }.get(self._state)
        onboarding_complete = self._published_steps == len(ONBOARDING_STEPS)
        action = ACTION.get(request.requested_state, "unknown")
        scenario = SCENARIOS[self._scenario_id]
        scenario_denied = action == scenario["deny_action"]
        approved = (
            onboarding_complete
            and request.requested_state == expected
            and not scenario_denied
        )
        previous = self._state
        self._decision_sequence += 1

        if approved:
            self._state = request.requested_state
            entitlement_id = f"ent-{self._decision_sequence:03d}-{action}"
            self._events.append(
                {
                    "kind": "evidence",
                    "code": "STAGE_EVIDENCE_SATISFIED",
                    "detail": f"{action}: {STAGE_EVIDENCE[self._state]}",
                }
            )
            self._entitlements.append(
                {
                    "id": entitlement_id,
                    "action": action,
                    "stage": STATE_NAME[self._state],
                    "ttl_s": 10 if self._state == DockingStatus.HARD_DOCK else 30,
                    "status": "consumed",
                }
            )
            reason_code = "ALLOW_POLICY_SATISFIED"
            detail = (
                f"action={action}; entitlement={entitlement_id}; audience=port-3; "
                f"ttl={'10' if self._state == DockingStatus.HARD_DOCK else '30'}s; replay=consumed"
            )
            summary = ACTION_SUMMARY[action]["allow"]
        elif scenario_denied:
            reason_code = scenario["reason"]
            detail = scenario["detail"]
            summary = scenario["summary"]
            self._events.append(
                {
                    "kind": "evidence",
                    "code": reason_code,
                    "detail": f"{action}: {detail}",
                    "summary": summary,
                }
            )
        else:
            reason_code = (
                "DENY_SESSION_NOT_AUTHORIZED"
                if not onboarding_complete
                else "DENY_INVALID_TRANSITION"
            )
            detail = "Authorization funnel incomplete" if not onboarding_complete else "Transition is not sequential"
            summary = "Waystation-1 denied the request because required authorization checks are incomplete."

        self._events.append(
            {
                "kind": "message",
                "code": reason_code,
                "detail": f"{action}: {detail}",
                "from": "WAYSTATION-1",
                "to": "ODYSSEY-7",
                "message_type": "TRANSITION_DECISION",
                "summary": summary,
            }
        )
        decision = TransitionDecision()
        decision.approved = approved
        decision.previous_state = previous
        decision.resulting_state = self._state
        decision.authority = "mock_policy_authority"
        decision.reason = reason_code
        self._decision_publisher.publish(decision)
        self._publish_status()
        self.get_logger().info(f"{action}: {reason_code}")

    def _on_reset(self, _message):
        self._state = DockingStatus.HOLD
        self._started_at = self.get_clock().now()
        self._published_steps = 0
        self._events = []
        self._entitlements = []
        self._decision_sequence = 0
        self._pending_request = None
        self._decision_due_at = None
        self._publish_status()
        self.get_logger().info("mock authorization workflow reset")

    def _on_scenario(self, message):
        if message.data not in SCENARIOS:
            self.get_logger().warning(f"ignored unknown scenario: {message.data}")
            return
        self._scenario_id = message.data
        self._publish_status()
        self.get_logger().info(f"selected authorization scenario: {message.data}")

    def _publish_status(self):
        completed_codes = [step[1] for step in ONBOARDING_STEPS[: self._published_steps]]
        payload = {
            "mode": "MOCK POLICY WORKFLOW",
            "scenario_id": self._scenario_id,
            "scenario": SCENARIOS[self._scenario_id]["name"],
            "station": "Waystation-1 / port-3",
            "chaser": "Odyssey-7",
            "operator": "Lunar Logistics",
            "session_id": "session:dock-2026-1842",
            "policy_id": "commercial-docking-v3",
            "trust_bundle": "waystation-1-trust@42",
            "phase": ONBOARDING_STEPS[self._published_steps - 1][1]
            if self._published_steps
            else "STARTING",
            "completed_steps": completed_codes,
            "events": self._events[-32:],
            "entitlements": self._entitlements[-6:],
        }
        message = String()
        message.data = json.dumps(payload, separators=(",", ":"))
        self._status_publisher.publish(message)


def main(args=None):
    rclpy.init(args=args)
    node = MockAuthorization()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()