from launch import LaunchDescription
from launch.actions import ExecuteProcess
from launch.substitutions import PathJoinSubstitution
from launch_ros.actions import Node
from launch_ros.substitutions import FindPackageShare


def generate_launch_description():
    world = PathJoinSubstitution(
        [FindPackageShare("docking_gazebo"), "worlds", "baseline_docking.sdf"]
    )

    gazebo = ExecuteProcess(
        cmd=["ign", "gazebo", "-r", "-s", world],
        output="screen",
    )
    bridge = Node(
        package="ros_gz_bridge",
        executable="parameter_bridge",
        arguments=["/world/docking/set_pose@ros_gz_interfaces/srv/SetEntityPose"],
        output="screen",
    )
    gate = Node(
        package="docking_orchestration",
        executable="mock_authorization",
        output="screen",
    )
    controller = Node(
        package="docking_orchestration",
        executable="baseline_controller",
        parameters=[
            {
                "approach_speed_mps": 0.18,
                "initial_delay_s": 19.0,
                "restart_delay_s": 19.0,
                "decision_timeout_s": 4.0,
            }
        ],
        output="screen",
    )
    dashboard = Node(
        package="docking_orchestration",
        executable="visual_dashboard",
        parameters=[{"port": 8080}],
        output="screen",
    )
    activity_logger = Node(
        package="docking_orchestration",
        executable="activity_logger",
        parameters=[{"telemetry_interval_s": 1.0}],
        output="screen",
    )

    return LaunchDescription(
        [
            gazebo,
            bridge,
            gate,
            dashboard,
            activity_logger,
            controller,
        ]
    )