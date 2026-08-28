FROM ros:humble-ros-base-jammy

SHELL ["/bin/bash", "-c"]

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-colcon-common-extensions \
    ros-humble-ros-gz-bridge \
    ros-humble-ros-gz-interfaces \
    ros-humble-ros-gz-sim \
    ros-humble-xacro \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY src /workspace/src

RUN source /opt/ros/humble/setup.bash \
    && colcon build --symlink-install \
    && rm -rf log

COPY docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["ros2", "launch", "docking_orchestration", "baseline_sim.launch.py"]