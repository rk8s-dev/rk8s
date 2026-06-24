#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(realpath "$SCRIPT_DIR/..")"
PROJECT_DIR="$(realpath "$DOCKER_DIR/../..")"

COMPOSE_FILE="$SCRIPT_DIR/docker-compose.etcd-perf.yml"
ARTIFACTS_DIR="$SCRIPT_DIR/artifacts"

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

usage() {
    cat <<EOF
用法: $(basename "$0") [选项]

说明:
  - 使用 docker compose 在容器内运行 xfstests 压力工具，元数据库为 etcd
  - 可选附带运行宿主机上的 slayerfs_bench
  - 测试产物输出到: $ARTIFACTS_DIR/perf-run-*

选项:
  --s3                       使用 rustfs 作为对象存储（SLAYERFS_DATA_BACKEND=s3）
  --tools "<tool...>"        指定压力工具列表，默认: "dirstress metaperf looptest"
  --slayerfs-bench           额外运行一次宿主机 cargo bench --bench slayerfs_bench
  --bench-args "<args...>"   透传给 cargo bench 之后的 Criterion 参数
  --keep                     结束后不执行 compose down（便于调试）
  -h, --help                 显示帮助

支持的 PERF_TOOLS:
  dirstress dirperf metaperf looptest

可通过环境变量覆盖各工具参数:
  PERF_DIRSTRESS_ARGS PERF_DIRPERF_ARGS PERF_METAPERF_ARGS PERF_LOOPTEST_ARGS
EOF
    exit 0
}

require_value() {
    local option="$1"
    local value="${2:-}"
    if [[ -z "$value" ]]; then
        err "$option 需要提供参数值"
        exit 1
    fi
}

KEEP=false
USE_S3=false
RUN_SLAYERFS_BENCH=false
PERF_TOOLS_VALUE="dirstress metaperf looptest"
BENCH_ARGS_VALUE=""

while [[ $# -gt 0 ]]; do
    case "${1:-}" in
        --s3)
            USE_S3=true
            shift
            ;;
        --tools)
            require_value "$1" "${2:-}"
            PERF_TOOLS_VALUE="${2:-}"
            shift 2
            ;;
        --slayerfs-bench)
            RUN_SLAYERFS_BENCH=true
            shift
            ;;
        --bench-args)
            require_value "$1" "${2:-}"
            BENCH_ARGS_VALUE="${2:-}"
            shift 2
            ;;
        --keep)
            KEEP=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            err "未知参数: $1"
            usage
            ;;
    esac
done

mkdir -p "$ARTIFACTS_DIR"

cleanup() {
    if [[ "$KEEP" == true ]]; then
        info "跳过 compose down (--keep)"
        return 0
    fi
    docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

run_slayerfs_bench() {
    local host_artifact_dir="$1"
    local bench_artifact_dir="$host_artifact_dir/slayerfs-bench"
    local benchmark_meta_urls="http://127.0.0.1:${ETCD_HOST_PORT:-12379}"
    local -a bench_args=()

    mkdir -p "$bench_artifact_dir"
    if [[ -n "$BENCH_ARGS_VALUE" ]]; then
        read -r -a bench_args <<<"$BENCH_ARGS_VALUE"
    fi

    info "运行宿主机 slayerfs_bench（etcd backend）"
    (
        cd "$PROJECT_DIR"
        env \
            RUST_LOG="${RUST_LOG:-warn}" \
            SLAYERFS_BENCH_META_BACKEND=etcd \
            SLAYERFS_BENCH_META_ETCD_URLS="$benchmark_meta_urls" \
            SLAYERFS_BENCH_BACKEND="$([[ "$USE_S3" == true ]] && echo s3 || echo local)" \
            SLAYERFS_BENCH_S3_BUCKET="${SLAYERFS_S3_BUCKET:-slayerfs-data}" \
            SLAYERFS_BENCH_S3_REGION="${SLAYERFS_S3_REGION:-us-east-1}" \
            SLAYERFS_BENCH_S3_ENDPOINT="http://127.0.0.1:${RUSTFS_S3_HOST_PORT:-19000}" \
            SLAYERFS_BENCH_S3_FORCE_PATH_STYLE=true \
            AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-rustfsadmin}" \
            AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-rustfsadmin}" \
            AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-us-east-1}" \
            cargo bench -p slayerfs --bench slayerfs_bench -- "${bench_args[@]}"
    ) 2>&1 | tee "$bench_artifact_dir/console.log"

    if [[ -d "$PROJECT_DIR/target/criterion" ]]; then
        rm -rf "$bench_artifact_dir/criterion"
        cp -a "$PROJECT_DIR/target/criterion" "$bench_artifact_dir/criterion" || true
    fi
}

info "构建宿主机 slayerfs release 二进制（供镜像 COPY）"
bash "$DOCKER_DIR/build_slayerfs_host_binary.sh"

info "构建 perf runner 镜像"
docker compose -f "$COMPOSE_FILE" build perf

ts="$(date +%s)-$RANDOM"
host_artifact_dir="$ARTIFACTS_DIR/perf-run-${ts}"
mkdir -p "$host_artifact_dir"

export SLAYERFS_ARTIFACT_DIR="/artifacts/perf-run-${ts}"
export SLAYERFS_S3_BUCKET="${SLAYERFS_S3_BUCKET:-slayerfs-data}"
if [[ "$USE_S3" == true ]]; then
    export SLAYERFS_DATA_BACKEND="s3"
else
    export SLAYERFS_DATA_BACKEND="local-fs"
fi

services=(etcd etcd-maintenance rustfs)
info "启动依赖服务: ${services[*]}"
docker compose -f "$COMPOSE_FILE" up -d "${services[@]}"

info "初始化 rustfs bucket（一次性容器）"
docker compose -f "$COMPOSE_FILE" run --rm rustfs-init

info "运行容器内性能测试（退出码由 perf 容器决定）"
set +e
docker compose -f "$COMPOSE_FILE" run --rm \
    -e PERF_TOOLS="$PERF_TOOLS_VALUE" \
    -e PERF_DIRSTRESS_ARGS \
    -e PERF_DIRPERF_ARGS \
    -e PERF_METAPERF_ARGS \
    -e PERF_LOOPTEST_ARGS \
    -e PERF_DIRSTRESS_PROCS \
    -e PERF_DIRSTRESS_FILES \
    -e PERF_DIRSTRESS_PROCS_PER_DIR \
    -e PERF_METAPERF_SECONDS \
    -e PERF_METAPERF_FILE_SIZE \
    -e PERF_METAPERF_OP_FILES \
    -e PERF_METAPERF_BG_FILES \
    -e PERF_LOOPTEST_ITERS \
    -e PERF_LOOPTEST_BUF_SIZE \
    perf
container_status=$?
set -e

bench_status=0
if [[ "$RUN_SLAYERFS_BENCH" == true ]]; then
    set +e
    run_slayerfs_bench "$host_artifact_dir"
    bench_status=$?
    set -e
fi

status=0
if [[ "$container_status" -ne 0 ]]; then
    status="$container_status"
fi
if [[ "$bench_status" -ne 0 ]]; then
    status="$bench_status"
fi

ok "perf compose 运行结束 (container=$container_status, bench=$bench_status)"
ok "产物目录: $host_artifact_dir"
exit "$status"
