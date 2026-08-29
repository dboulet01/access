import math

import rclpy
from docking_interfaces.msg import DockingStatus, TransitionDecision, TransitionRequest
from geometry_msgs.msg import Pose
from rclpy.node import Node
from ros_gz_interfaces.msg import Entity
from ros_gz_interfaces.srv import SetEntityPose
from std_msgs.msg import Empty, String


GOAL_X = {
    DockingStatus.HOLD: -5.0,
    DockingStatus.APPROACH: -2.79,
    DockingStatus.FINAL_APPROACH: -1.99,
    DockingStatus.SOFT_CAPTURE: -1.71,
    DockingStatus.HARD_DOCK: -1.68,
}

MAX_STAGE_SPEED_MPS = {
    DockingStatus.HOLD: 0.0,
    DockingStatus.APPROACH: 0.18,
    DockingStatus.FINAL_APPROACH: 0.08,
    DockingStatus.SOFT_CAPTURE: 0.02,
    DockingStatus.HARD_DOCK: 0.01,
}

NEXT_STATE = {
    DockingStatus.HOLD: DockingStatus.APPROACH,
    DockingStatus.APPROACH: DockingStatus.FINAL_APPROACH,
    DockingStatus.FINAL_APPROACH: DockingStatus.SOFT_CAPTURE,
    DockingStatus.SOFT_CAPTURE: DockingStatus.HARD_DOCK,
}

STATE_NAME = {
    DockingStatus.HOLD: "HOLD",
    DockingStatus.APPROACH: "APPROACH",
    DockingStatus.FINAL_APPROACH: "FINAL_APPROACH",
    DockingStatus.SOFT_CAPTURE: "SOFT_CAPTURE",
    DockingStatus.HARD_DOCK: "HARD_DOCK",
}


class BaselineController(Node):
    """Deterministic kinematic RPOD baseline with replaceable transition authority."""

    def __init__(self):
        super().__init__("baseline_controller")
        self.declare_parameter("update_rate_hz", 20.0)
        self.declare_parameter("approach_speed_mps", 0.6)
        self.declare_parameter("restart_delay_s", 3.0)
        self.declare_parameter("initial_delay_s", 0.0)
        self.declare_parameter("decision_timeout_s", 5.0)
        self._rate = self.get_parameter("update_rate_hz").value
        self._speed = self.get_parameter("approach_speed_mps").value
        self._restart_delay_s = self.get_parameter("restart_delay_s").value
        self._decision_timeout_s = self.get_parameter("decision_timeout_s").value
        self._state = DockingStatus.HOLD
        self._x = GOAL_X[DockingStatus.HOLD]
        self._pending = False
        self._request_time = None
        self._pose_call = None
        self._pending_x = None
        self._reset_requested = False
        self._reset_pose_pending = False
        self._start_after_reset = False
        self._halted = True
        initial_delay_s = self.get_parameter("initial_delay_s").value
        self._resume_time = (
            self.get_clock().now()
            + rclpy.duration.Duration(seconds=initial_delay_s)
            if initial_delay_s > 0.0
            else None
        )

        self._status_publisher = self.create_publisher(
            DockingStatus, "/docking/status", 10
        )
        self._request_publisher = self.create_publisher(
            TransitionRequest, "/docking/transition_request", 10
        )
        self.create_subscription(
            TransitionDecision, "/docking/transition_decision", self._on_decision, 10
        )
        self.create_subscription(Empty, "/docking/reset", self._on_reset, 10)
        self.create_subscription(Empty, "/docking/prepare", self._on_prepare, 10)
        self.create_subscription(String, "/docking/run", self._on_reset, 10)
        self._pose_client = self.create_client(
            SetEntityPose, "/world/docking/set_pose"
        )
        self.create_timer(1.0 / self._rate, self._update)

    def _on_reset(self, _message):
        self._request_reset(start_after_reset=True)
        self.get_logger().info("simulation start requested")

    def _on_prepare(self, _message):
        self._request_reset(start_after_reset=False)
        self.get_logger().info("simulation prepared at HOLD")

    def _request_reset(self, start_after_reset):
        self._reset_requested = True
        self._start_after_reset = start_after_reset
        self._halted = not start_after_reset
        self._pending = False
        self._request_time = None
        self._resume_time = None

    def _on_decision(self, decision):
        if not self._pending:
            return
        requested = NEXT_STATE.get(self._state)
        if decision.approved and decision.resulting_state == requested:
            self._state = decision.resulting_state
            self.get_logger().info(
                f"entered {STATE_NAME[self._state]}: authority={decision.authority}, "
                f"reason={decision.reason}"
            )
        else:
            self._halted = True
            self.get_logger().warn(
                f"transition denied by {decision.authority}: {decision.reason}"
            )
        self._pending = False
        self._request_time = None

    def _request_transition(self):
        requested = NEXT_STATE.get(self._state)
        if requested is None:
            return
        request = TransitionRequest()
        request.requested_state = requested
        request.requester = self.get_name()
        request.reason = f"range condition met for {STATE_NAME[requested]}"
        self._request_publisher.publish(request)
        self._pending = True
        self._request_time = self.get_clock().now()

    def _advance_pose(self):
        if not self._pose_client.service_is_ready():
            return
        if self._pose_call is not None:
            if not self._pose_call.done():
                return
            try:
                response = self._pose_call.result()
            except Exception as error:
                self.get_logger().error(f"Gazebo pose request failed: {error}")
            else:
                if response.success:
                    self._x = self._pending_x
                    if self._reset_pose_pending:
                        self._state = DockingStatus.HOLD
                        if self._start_after_reset:
                            self._resume_time = (
                                self.get_clock().now()
                                + rclpy.duration.Duration(seconds=self._restart_delay_s)
                            )
                        else:
                            self._resume_time = None
                else:
                    self.get_logger().error("Gazebo rejected chaser pose request")
                    if self._reset_pose_pending:
                        self._reset_requested = True
            self._reset_pose_pending = False
            self._pose_call = None
            self._pending_x = None

        if self._reset_requested:
            request = SetEntityPose.Request()
            request.entity = Entity(name="chaser", type=Entity.MODEL)
            request.pose = Pose()
            request.pose.position.x = GOAL_X[DockingStatus.HOLD]
            request.pose.orientation.w = 1.0
            self._pending_x = GOAL_X[DockingStatus.HOLD]
            self._pose_call = self._pose_client.call_async(request)
            self._reset_requested = False
            self._reset_pose_pending = True
            return

        goal = GOAL_X[self._state]
        stage_speed = min(self._speed, MAX_STAGE_SPEED_MPS[self._state])
        step = stage_speed / self._rate
        next_x = self._x + math.copysign(min(abs(goal - self._x), step), goal - self._x)
        request = SetEntityPose.Request()
        request.entity = Entity(name="chaser", type=Entity.MODEL)
        request.pose = Pose()
        request.pose.position.x = next_x
        request.pose.orientation.w = 1.0
        self._pending_x = next_x
        self._pose_call = self._pose_client.call_async(request)

    def _publish_status(self):
        status = DockingStatus()
        status.stamp = self.get_clock().now().to_msg()
        status.state = self._state
        status.range_m = max(0.0, -1.68 - self._x)
        status.transition_pending = self._pending
        status.detail = STATE_NAME[self._state]
        self._status_publisher.publish(status)

    def _update(self):
        if self._pending and self._request_time is not None:
            elapsed = (self.get_clock().now() - self._request_time).nanoseconds / 1e9
            if elapsed > self._decision_timeout_s:
                self._halted = True
                self._pending = False
                self._request_time = None
                self.get_logger().error("transition authority response timed out; halted")

        goal = GOAL_X[self._state]
        self._advance_pose()
        if self._reset_requested or self._reset_pose_pending:
            return
        self._publish_status()
        if self._resume_time is not None:
            if self.get_clock().now() < self._resume_time:
                return
            self._resume_time = None
        if abs(goal - self._x) < 1e-6 and not self._pending and not self._halted:
            self._request_transition()


def main(args=None):
    rclpy.init(args=args)
    node = BaselineController()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()