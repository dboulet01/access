import json
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import rclpy
from ament_index_python.packages import get_package_share_directory
from docking_interfaces.msg import DockingStatus, TransitionDecision, TransitionRequest
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import Empty, String


STATE_NAME = {
    DockingStatus.HOLD: "HOLD",
    DockingStatus.APPROACH: "APPROACH",
    DockingStatus.FINAL_APPROACH: "FINAL APPROACH",
    DockingStatus.SOFT_CAPTURE: "SOFT CAPTURE",
    DockingStatus.HARD_DOCK: "HARD DOCK",
    DockingStatus.ABORTED: "ABORTED",
}

SCENARIO_IDS = {
    "nominal",
    "expired_credential",
    "corridor_violation",
    "latch_not_ready",
}


class DashboardState:
    def __init__(self):
        self.lock = threading.Lock()
        self.snapshot = {
            "state": "HOLD",
            "state_id": int(DockingStatus.HOLD),
            "range_m": 3.32,
            "transition_pending": False,
            "events": [],
            "authorization": {
                "mode": "WAITING FOR AUTHORITY",
                "scenario": "Commercial methane refueling",
                "station": "Waystation-1 / port-3",
                "chaser": "Odyssey-7",
                "operator": "Lunar Logistics",
                "session_id": "pending",
                "policy_id": "commercial-docking-v3",
                "trust_bundle": "waystation-1-trust@42",
                "phase": "IDLE",
                "completed_steps": [],
                "events": [],
                "entitlements": [],
            },
        }
        self.rerun_requested = threading.Event()
        self.prepare_requested = threading.Event()
        self.requested_scenario = "nominal"

    def update_status(self, status):
        with self.lock:
            self.snapshot.update(
                state=STATE_NAME.get(status.state, str(status.state)),
                state_id=int(status.state),
                range_m=round(status.range_m, 4),
                transition_pending=bool(status.transition_pending),
            )

    def add_event(self, kind, text):
        with self.lock:
            self.snapshot["events"].append({"kind": kind, "text": text})
            self.snapshot["events"] = self.snapshot["events"][-12:]

    def update_authorization(self, authorization):
        with self.lock:
            self.snapshot["authorization"] = authorization

    def encode(self):
        with self.lock:
            return json.dumps(self.snapshot).encode("utf-8")

    def request_rerun(self, scenario):
        with self.lock:
            self.requested_scenario = scenario
            self.snapshot.update(
                state="RESETTING",
                state_id=-1,
                range_m=3.32,
                transition_pending=False,
                events=[{"kind": "request", "text": "Validation run requested"}],
            )
        self.rerun_requested.set()

    def request_prepare(self):
        with self.lock:
            self.snapshot.update(
                state="HOLD",
                state_id=int(DockingStatus.HOLD),
                range_m=3.32,
                transition_pending=False,
                events=[],
            )
        self.prepare_requested.set()


def handler_for(web_root, state):
    class DashboardHandler(SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(web_root), **kwargs)

        def do_GET(self):
            if self.path == "/api/state":
                payload = state.encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            super().do_GET()

        def do_POST(self):
            if self.path == "/api/prepare":
                state.request_prepare()
                payload = json.dumps({"accepted": True}).encode()
                self.send_response(202)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            if self.path != "/api/rerun":
                self.send_error(404)
                return
            try:
                content_length = int(self.headers.get("Content-Length", "0"))
                request = json.loads(self.rfile.read(content_length) or b"{}")
            except (ValueError, json.JSONDecodeError):
                self.send_error(400, "Invalid JSON body")
                return
            scenario = request.get("scenario", "nominal")
            if scenario not in SCENARIO_IDS:
                self.send_error(400, "Unknown scenario")
                return
            state.request_rerun(scenario)
            payload = json.dumps({"accepted": True, "scenario": scenario}).encode()
            self.send_response(202)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, _format, *_args):
            pass

    return DashboardHandler


class VisualDashboard(Node):
    def __init__(self):
        super().__init__("docking_visual_dashboard")
        self.declare_parameter("port", 8080)
        self._state = DashboardState()
        self.create_subscription(DockingStatus, "/docking/status", self._on_status, 10)
        self.create_subscription(
            TransitionRequest, "/docking/transition_request", self._on_request, 10
        )
        self.create_subscription(
            TransitionDecision, "/docking/transition_decision", self._on_decision, 10
        )
        authorization_qos = QoSProfile(
            depth=1,
            reliability=ReliabilityPolicy.RELIABLE,
            durability=DurabilityPolicy.TRANSIENT_LOCAL,
        )
        self.create_subscription(
            String,
            "/authorization/status",
            self._on_authorization,
            authorization_qos,
        )
        self._run_publisher = self.create_publisher(String, "/docking/run", 10)
        self._prepare_publisher = self.create_publisher(Empty, "/docking/prepare", 10)
        self.create_timer(0.05, self._publish_requested_reset)

        web_root = Path(get_package_share_directory("docking_orchestration")) / "web"
        port = self.get_parameter("port").value
        self._server = ThreadingHTTPServer(("0.0.0.0", port), handler_for(web_root, self._state))
        self._server_thread = threading.Thread(
            target=self._server.serve_forever, daemon=True
        )
        self._server_thread.start()
        self.get_logger().info(f"visual dashboard ready on http://localhost:{port}")

    def _publish_requested_reset(self):
        if self._state.prepare_requested.is_set():
            self._state.prepare_requested.clear()
            self._prepare_publisher.publish(Empty())
            self.get_logger().info("published simulation prepare command")
        if not self._state.rerun_requested.is_set():
            return
        self._state.rerun_requested.clear()
        scenario = String()
        scenario.data = self._state.requested_scenario
        self._run_publisher.publish(scenario)
        self.get_logger().info(
            f"published simulation reset command: scenario={scenario.data}"
        )

    def _on_status(self, status):
        self._state.update_status(status)

    def _on_request(self, request):
        requested = STATE_NAME.get(request.requested_state, str(request.requested_state))
        self._state.add_event(
            "request",
            f"{request.requester} requested {requested}: {request.reason}",
        )

    def _on_decision(self, decision):
        resulting = STATE_NAME.get(decision.resulting_state, str(decision.resulting_state))
        outcome = "Approved" if decision.approved else "Denied"
        self._state.add_event(
            "approved" if decision.approved else "denied",
            f"{outcome} by {decision.authority}: {resulting} ({decision.reason})",
        )

    def _on_authorization(self, message):
        try:
            authorization = json.loads(message.data)
        except (json.JSONDecodeError, TypeError):
            self.get_logger().error("ignored malformed authorization status")
            return
        self._state.update_authorization(authorization)

    def destroy_node(self):
        self._server.shutdown()
        self._server.server_close()
        super().destroy_node()


def main(args=None):
    rclpy.init(args=args)
    node = VisualDashboard()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()