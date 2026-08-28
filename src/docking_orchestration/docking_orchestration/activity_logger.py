import rclpy
from docking_interfaces.msg import DockingStatus, TransitionDecision, TransitionRequest
from rclpy.node import Node


STATE_NAME = {
    DockingStatus.HOLD: "HOLD",
    DockingStatus.APPROACH: "APPROACH",
    DockingStatus.FINAL_APPROACH: "FINAL_APPROACH",
    DockingStatus.SOFT_CAPTURE: "SOFT_CAPTURE",
    DockingStatus.HARD_DOCK: "HARD_DOCK",
    DockingStatus.ABORTED: "ABORTED",
}


class ActivityLogger(Node):
    """Human-readable observer for docking telemetry and authority events."""

    def __init__(self):
        super().__init__("docking_activity_logger")
        self.declare_parameter("telemetry_interval_s", 0.5)
        self._telemetry_interval_s = self.get_parameter(
            "telemetry_interval_s"
        ).value
        self._last_status_time = None
        self._last_state = None
        self._completion_reported = False
        self.create_subscription(DockingStatus, "/docking/status", self._on_status, 10)
        self.create_subscription(
            TransitionRequest, "/docking/transition_request", self._on_request, 10
        )
        self.create_subscription(
            TransitionDecision, "/docking/transition_decision", self._on_decision, 10
        )
        self.get_logger().info(
            f"activity logging enabled every {self._telemetry_interval_s:.2f}s"
        )

    def _on_status(self, status):
        now = self.get_clock().now()
        docking_complete = (
            status.state == DockingStatus.HARD_DOCK and status.range_m <= 1e-3
        )
        if docking_complete and self._completion_reported:
            return

        state_changed = status.state != self._last_state
        interval_elapsed = (
            self._last_status_time is None
            or (now - self._last_status_time).nanoseconds / 1e9
            >= self._telemetry_interval_s
        )
        if not docking_complete and not state_changed and not interval_elapsed:
            return

        event = "COMPLETE" if docking_complete else "STATUS"
        self.get_logger().info(
            f"{event} state={STATE_NAME.get(status.state, status.state)}, "
            f"range={status.range_m:.3f}m, "
            f"transition_pending={status.transition_pending}"
        )
        self._last_state = status.state
        self._last_status_time = now
        self._completion_reported = docking_complete

    def _on_request(self, request):
        self.get_logger().info(
            f"REQUEST requester={request.requester}, "
            f"requested_state={STATE_NAME.get(request.requested_state, request.requested_state)}, "
            f"reason={request.reason}"
        )

    def _on_decision(self, decision):
        outcome = "APPROVED" if decision.approved else "DENIED"
        self.get_logger().info(
            f"DECISION outcome={outcome}, authority={decision.authority}, "
            f"state={STATE_NAME.get(decision.previous_state, decision.previous_state)}"
            f"->{STATE_NAME.get(decision.resulting_state, decision.resulting_state)}, "
            f"reason={decision.reason}"
        )


def main(args=None):
    rclpy.init(args=args)
    node = ActivityLogger()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()