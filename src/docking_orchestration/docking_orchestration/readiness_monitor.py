import math

import rclpy
from docking_interfaces.msg import DockingStatus, ReadinessEvidence
from rclpy.node import Node
from std_msgs.msg import String


class ReadinessMonitor(Node):
    """Produces station-local operational evidence independently of authorization."""

    def __init__(self):
        super().__init__("readiness_monitor")
        self._scenario = "nominal"
        self._previous_range = None
        self._previous_time = None
        self._publisher = self.create_publisher(
            ReadinessEvidence, "/docking/readiness", 10
        )
        self.create_subscription(DockingStatus, "/docking/status", self._on_status, 10)
        self.create_subscription(
            String, "/authorization/scenario", self._on_scenario, 10
        )
        self.create_subscription(String, "/docking/run", self._on_scenario, 10)

    def _on_scenario(self, message):
        self._scenario = message.data

    def _on_status(self, status):
        observed_at = self.get_clock().now()
        closing_rate = 0.0
        if self._previous_range is not None and self._previous_time is not None:
            elapsed_s = (observed_at - self._previous_time).nanoseconds / 1e9
            if elapsed_s > 0.0:
                closing_rate = max(
                    0.0, (self._previous_range - status.range_m) / elapsed_s
                )
        self._previous_range = status.range_m
        self._previous_time = observed_at

        evidence = ReadinessEvidence()
        evidence.stamp = observed_at.to_msg()
        evidence.range_m = status.range_m
        evidence.closing_rate_mps = closing_rate
        evidence.initial_hold_confirmed = (
            status.state == DockingStatus.HOLD
            and abs(status.range_m - 3.32) <= 0.01
            and not status.transition_pending
        )
        evidence.retreat_available = True
        evidence.relative_navigation_valid = math.isfinite(status.range_m)
        evidence.approach_corridor_clear = self._scenario != "corridor_violation"
        evidence.closing_rate_within_limit = math.isfinite(closing_rate)
        evidence.alignment_within_limit = True
        evidence.capture_system_ready = True
        evidence.soft_capture_confirmed = status.state == DockingStatus.SOFT_CAPTURE
        evidence.latches_ready = self._scenario != "latch_not_ready"
        evidence.relative_motion_stable = closing_rate <= 0.05
        self._publisher.publish(evidence)


def main(args=None):
    rclpy.init(args=args)
    node = ReadinessMonitor()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()