from launch import LaunchDescription
from launch.actions import DeclareLaunchArgument, ExecuteProcess, RegisterEventHandler, Shutdown
from launch.conditions import IfCondition
from launch.event_handlers import OnProcessExit
from launch.substitutions import LaunchConfiguration, PathJoinSubstitution
from launch_ros.actions import Node
from launch_ros.substitutions import FindPackageShare


def generate_launch_description():
    world = PathJoinSubstitution(
        [FindPackageShare("docking_gazebo"), "worlds", "baseline_docking.sdf"]
    )
    smoke_test = LaunchConfiguration("smoke_test")
    verbose = LaunchConfiguration("verbose")
    telemetry_interval = LaunchConfiguration("telemetry_interval")

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
        executable="development_gate",
        output="screen",
    )
    controller = Node(
        package="docking_orchestration",
        executable="baseline_controller",
        output="screen",
    )
    monitor = Node(
        package="docking_orchestration",
        executable="docking_monitor",
        condition=IfCondition(smoke_test),
        output="screen",
    )
    activity_logger = Node(
        package="docking_orchestration",
        executable="activity_logger",
        condition=IfCondition(verbose),
        parameters=[{"telemetry_interval_s": telemetry_interval}],
        output="screen",
    )

    return LaunchDescription(
        [
            DeclareLaunchArgument("smoke_test", default_value="false"),
            DeclareLaunchArgument("verbose", default_value="false"),
            DeclareLaunchArgument("telemetry_interval", default_value="0.5"),
            gazebo,
            bridge,
            gate,
            controller,
            monitor,
            activity_logger,
            RegisterEventHandler(
                OnProcessExit(
                    target_action=monitor,
                    on_exit=[Shutdown(reason="baseline docking smoke test completed")],
                )
            ),
        ]
    )