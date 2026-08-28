import rclpy
from docking_interfaces.msg import DockingStatus
from rclpy.node import Node


class HardDockReached(Exception):
    pass


class DockingMonitor(Node):
    def __init__(self):
        super().__init__("docking_monitor")
        self.create_subscription(DockingStatus, "/docking/status", self._on_status, 10)

    def _on_status(self, status):
        if status.state == DockingStatus.HARD_DOCK and status.range_m <= 1e-3:
            self.get_logger().info("HARD_DOCK_REACHED")
            raise HardDockReached


def main(args=None):
    rclpy.init(args=args)
    node = DockingMonitor()
    try:
        rclpy.spin(node)
    except HardDockReached:
        pass
    finally:
        node.destroy_node()
        rclpy.shutdown()