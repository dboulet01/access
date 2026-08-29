from glob import glob
from setuptools import find_packages, setup


package_name = "docking_orchestration"

setup(
    name=package_name,
    version="0.1.0",
    packages=find_packages(exclude=["test"]),
    data_files=[
        ("share/ament_index/resource_index/packages", ["resource/" + package_name]),
        ("share/" + package_name, ["package.xml"]),
        ("share/" + package_name + "/launch", glob("launch/*.launch.py")),
        ("share/" + package_name + "/web", glob("web/*")),
    ],
    install_requires=["setuptools"],
    zip_safe=True,
    maintainer="Project Maintainers",
    maintainer_email="maintainer@example.com",
    description="Spacecraft authorization docking reference orchestration.",
    license="Apache-2.0",
    entry_points={
        "console_scripts": [
            "activity_logger = docking_orchestration.activity_logger:main",
            "chaser_access = docking_orchestration.chaser_access:main",
            "baseline_controller = docking_orchestration.baseline_controller:main",
            "docking_monitor = docking_orchestration.docking_monitor:main",
            "readiness_monitor = docking_orchestration.readiness_monitor:main",
            "session_gateway = docking_orchestration.session_gateway:main",
            "station_access = docking_orchestration.station_access:main",
            "visual_dashboard = docking_orchestration.visual_dashboard:main",
        ],
    },
)