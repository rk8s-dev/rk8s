#!/usr/bin/env bash
set -euo pipefail

current_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
slayerfs_dir=$(cd "$current_dir/../.." && pwd)
repo_root=$(cd "$slayerfs_dir/../.." && pwd)
artifacts_dir="$current_dir/artifacts"
image_name="slayerfs-xfstests-runner:local"

mkdir -p "$artifacts_dir"

echo "[1/3] Building xfstests runner image..."
docker build -t "$image_name" -f "$current_dir/Dockerfile" "$current_dir"

echo "[2/3] Running Redis xfstests in container..."
set +e
docker run --rm --privileged --network host \
  -e XFSTESTS_CASES="${XFSTESTS_CASES:-generic/001 generic/013 generic/023 generic/035}" \
  -e XFSTESTS_BRANCH="${XFSTESTS_BRANCH:-v2023.12.10}" \
  -v "$repo_root:/workspace/rk8s" \
  -v "$artifacts_dir:/artifacts" \
  "$image_name" \
  bash -lc '
    set -euo pipefail
    export PATH="/usr/local/cargo/bin:/usr/local/rustup/bin:$PATH"
    cd /workspace/rk8s/project/slayerfs

    cargo build -p slayerfs --example persistence_demo --release

    export SLAYERFS_CONFIG=/workspace/rk8s/project/slayerfs/redis.yml
    export slayerfs_rust_log="${slayerfs_rust_log:-slayerfs=info,rfuse3::raw::logfs=warn}"
    export slayerfs_fuse_op_log="${slayerfs_fuse_op_log:-0}"

    set +e
    bash tests/scripts/xfstests_slayer.sh
    run_status=$?

    cp -f /tmp/slayerfs.log /artifacts/slayerfs.log 2>/dev/null || true
    cp -rf /tmp/xfstests-dev/results /artifacts/results 2>/dev/null || true
    cp -f /tmp/xfstests-dev/check.log /artifacts/check.log 2>/dev/null || true

    exit $run_status
  '
run_status=$?
set -e

echo "[3/3] Collecting artifacts..."
echo "Artifacts written to: $artifacts_dir"

if [[ $run_status -ne 0 ]]; then
  echo "xfstests exited with status $run_status"
  echo "See artifacts under: $artifacts_dir"
  exit $run_status
fi

echo "xfstests completed. Artifacts: $artifacts_dir"
