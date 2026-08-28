$ErrorActionPreference = "Stop"

docker compose build docking-sim
if ($LASTEXITCODE -ne 0) {
    throw "Simulation image build failed"
}

$rosDomainId = Get-Random -Minimum 100 -Maximum 200
$ignitionPartition = "docking_smoke_$([guid]::NewGuid().ToString('N'))"
$runCommand = "(sleep 5; ros2 topic pub --once /docking/reset std_msgs/msg/Empty '{}') & ros2 launch docking_orchestration baseline_sim.launch.py smoke_test:=true"
docker compose run --rm `
    -e ROS_DOMAIN_ID=$rosDomainId `
    -e IGN_PARTITION=$ignitionPartition `
    docking-sim `
    bash -lc $runCommand
if ($LASTEXITCODE -ne 0) {
    throw "Docking smoke test failed"
}