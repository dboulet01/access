import json
import uuid

import rclpy
from docking_interfaces.msg import TransitionRequest
from rclpy.node import Node
from std_msgs.msg import Empty, String


SCENARIOS = {
    "nominal",
    "expired_credential",
    "corridor_violation",
    "latch_not_ready",
}


class ChaserAccess(Node):
    """Chaser-side ACCESS participant that exchanges protocol material with station ACCESS."""

    def __init__(self):
        super().__init__("chaser_access")
        self._scenario_id = "nominal"
        self._session_id = None
        self._sequence = 0

        self._protocol_publisher = self.create_publisher(
            String, "/access/chaser_to_station", 10
        )
        self.create_subscription(
            String, "/access/station_to_chaser", self._on_station_protocol, 10
        )
        self.create_subscription(
            TransitionRequest, "/docking/transition_request", self._on_transition_request, 10
        )
        self.create_subscription(Empty, "/docking/reset", self._on_reset, 10)
        self.create_subscription(Empty, "/docking/prepare", self._on_prepare, 10)
        self.create_subscription(String, "/docking/run", self._on_run, 10)
        self.create_subscription(
            String, "/authorization/scenario", self._on_scenario, 10
        )
        self.get_logger().info(
            "chaser ACCESS node ready; secure transport assumed by mission comms layer"
        )

    def _on_scenario(self, message):
        if message.data in SCENARIOS:
            self._scenario_id = message.data

    def _on_prepare(self, _message):
        self._session_id = None
        self._sequence = 0

    def _on_reset(self, _message):
        self._start_identity_exchange("reset")

    def _on_run(self, message):
        if message.data in SCENARIOS:
            self._scenario_id = message.data
        self._start_identity_exchange("run")

    def _start_identity_exchange(self, trigger):
        self._session_id = None
        self._sequence = 0
        payload = {
            "protocol_version": "1.0",
            "kind": "access_request",
            "message_id": f"msg-{uuid.uuid4().hex}",
            "from": "ODYSSEY-7",
            "to": "WAYSTATION-1",
            "scenario_id": self._scenario_id,
            "trigger": trigger,
            "secure_transport_assumed": True,
            "credential_presentation_profile": {
                "model": "W3C VC 2.0",
                "presentation_format": "application/vp+json",
                "credential_types": [
                    "VehicleRegistrationCredential",
                    "DockingCertificationCredential",
                ],
                "subject": "did:web:lunar-logistics.example:spacecraft:odyssey-7",
            },
        }
        self._publish_protocol(payload)

    def _on_transition_request(self, request):
        self._sequence += 1
        payload = {
            "protocol_version": "1.0",
            "kind": "transition_request",
            "message_id": f"msg-{uuid.uuid4().hex}",
            "sequence": self._sequence,
            "from": "ODYSSEY-7",
            "to": "WAYSTATION-1",
            "session_id": self._session_id,
            "requested_state": int(request.requested_state),
            "requester": request.requester,
            "reason": request.reason,
            "secure_transport_assumed": True,
        }
        self._publish_protocol(payload)

    def _on_station_protocol(self, message):
        try:
            payload = json.loads(message.data)
        except (json.JSONDecodeError, TypeError):
            self.get_logger().warning("ignored malformed station protocol message")
            return
        kind = payload.get("kind")
        if kind == "session_authorized":
            self._session_id = payload.get("session_id")
            self.get_logger().info(
                f"session authorized by station: session_id={self._session_id}"
            )
        elif kind == "session_denied":
            self._session_id = None
            self.get_logger().warning(
                f"session denied by station: reason={payload.get('reason', 'unknown')}"
            )
        elif kind == "transition_decision":
            self.get_logger().info(
                "received station transition decision: "
                f"approved={payload.get('approved')} reason={payload.get('reason')}"
            )

    def _publish_protocol(self, payload):
        message = String()
        message.data = json.dumps(payload, separators=(",", ":"))
        self._protocol_publisher.publish(message)


def main(args=None):
    rclpy.init(args=args)
    node = ChaserAccess()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()
