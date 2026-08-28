import rclpy
from docking_interfaces.msg import DockingStatus, TransitionDecision, TransitionRequest
from rclpy.node import Node
from std_msgs.msg import Empty


class DevelopmentGate(Node):
    """Sequential transition authority used only by the unsecured baseline."""

    def __init__(self):
        super().__init__("development_gate")
        self._state = DockingStatus.HOLD
        self._publisher = self.create_publisher(
            TransitionDecision, "/docking/transition_decision", 10
        )
        self.create_subscription(
            TransitionRequest, "/docking/transition_request", self._on_request, 10
        )
        self.create_subscription(Empty, "/docking/reset", self._on_reset, 10)
        self.create_subscription(Empty, "/docking/prepare", self._on_reset, 10)

    def _on_reset(self, _message):
        self._state = DockingStatus.HOLD
        self.get_logger().info("transition authority reset to HOLD")

    def _on_request(self, request):
        expected = {
            DockingStatus.HOLD: DockingStatus.APPROACH,
            DockingStatus.APPROACH: DockingStatus.FINAL_APPROACH,
            DockingStatus.FINAL_APPROACH: DockingStatus.SOFT_CAPTURE,
            DockingStatus.SOFT_CAPTURE: DockingStatus.HARD_DOCK,
        }.get(self._state)
        approved = request.requested_state == expected
        previous = self._state
        if approved:
            self._state = request.requested_state

        decision = TransitionDecision()
        decision.approved = approved
        decision.previous_state = previous
        decision.resulting_state = self._state
        decision.authority = "development_gate"
        decision.reason = (
            "baseline sequential policy"
            if approved
            else "transition is not the next sequential state"
        )
        self._publisher.publish(decision)
        self.get_logger().info(
            f"transition {previous} -> {request.requested_state}: "
            f"{'approved' if approved else 'denied'}"
        )


def main(args=None):
    rclpy.init(args=args)
    node = DevelopmentGate()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()