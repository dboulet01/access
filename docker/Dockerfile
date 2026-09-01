FROM rust:1.89-bullseye AS rust-builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY rust ./rust
RUN cargo build --locked --release --bin access-authority --bin access-signer --bin access-entitlement-verifier

FROM ros:humble-ros-base-jammy

SHELL ["/bin/bash", "-c"]

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-colcon-common-extensions \
    ros-humble-ros-gz-bridge \
    ros-humble-ros-gz-interfaces \
    ignition-fortress \
    ros-humble-xacro \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY src /workspace/src
COPY config /workspace/config
COPY examples /workspace/examples
COPY --from=rust-builder /build/target/release/access-authority /usr/local/bin/access-authority
COPY --from=rust-builder /build/target/release/access-signer /usr/local/bin/access-signer
COPY --from=rust-builder /build/target/release/access-entitlement-verifier /usr/local/bin/access-entitlement-verifier

RUN source /opt/ros/humble/setup.bash \
    && colcon build --symlink-install \
    && rm -rf log

COPY docker/entrypoint.sh /entrypoint.sh
RUN sed -i 's/\r$//' /entrypoint.sh && chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["ros2", "launch", "docking_orchestration", "baseline_sim.launch.py"]