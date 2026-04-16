#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(realpath "$SCRIPT_DIR/..")"
PROJECT_DIR="$(realpath "$DOCKER_DIR/../..")"

COMPOSE_FILE="$SCRIPT_DIR/docker-compose.redis.yml"
ARTIFACTS_DIR="$SCRIPT_DIR/artifacts"

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

usage() {
    cat <<EOF
用法: $(basename "$0") [选项]

说明:
  - 使用 docker compose 在容器内运行 xfstests（FUSE），元数据库为 redis
  - 测试产物输出到: $ARTIFACTS_DIR

选项:
  --s3                       使用 rustfs 作为对象存储（SLAYERFS_DATA_BACKEND=s3）
  --cases "<case...>"        只跑指定用例，例如: "generic/001 generic/002"
  --skip-cases <N>           全量模式下跳过默认测试序列中的前 N 个用例
  --check-args "<args...>"   直接透传给 xfstests ./check 的参数
  --keep                     结束后不执行 compose down（便于调试）
  -h, --help                 显示帮助
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

require_non_negative_integer() {
    local option="$1"
    local value="${2:-}"
    if ! [[ "$value" =~ ^[0-9]+$ ]]; then
        err "$option 需要提供非负整数，当前值: ${value:-<empty>}"
        exit 1
    fi
}

KEEP=false
USE_S3=false
XFSTESTS_CASES_VALUE=""
XFSTESTS_CHECK_ARGS_VALUE=""
XFSTESTS_SKIP_CASES_VALUE="0"

while [[ $# -gt 0 ]]; do
    case "${1:-}" in
        --s3)
            USE_S3=true
            shift
            ;;
        --cases)
            require_value "$1" "${2:-}"
            XFSTESTS_CASES_VALUE="${2:-}"
            shift 2
            ;;
        --skip-cases)
            require_value "$1" "${2:-}"
            require_non_negative_integer "$1" "${2:-}"
            XFSTESTS_SKIP_CASES_VALUE="${2:-}"
            shift 2
            ;;
        --check-args)
            require_value "$1" "${2:-}"
            XFSTESTS_CHECK_ARGS_VALUE="${2:-}"
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

if [[ "$XFSTESTS_SKIP_CASES_VALUE" != "0" && -n "$XFSTESTS_CASES_VALUE" ]]; then
    err "--skip-cases 不能与 --cases 同时使用"
    exit 1
fi

if [[ "$XFSTESTS_SKIP_CASES_VALUE" != "0" && -n "$XFSTESTS_CHECK_ARGS_VALUE" ]]; then
    err "--skip-cases 不能与 --check-args 同时使用"
    exit 1
fi

mkdir -p "$ARTIFACTS_DIR"

cleanup() {
    if [[ "$KEEP" == true ]]; then
        info "跳过 compose down (--keep)"
        return 0
    fi
    docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

info "构建宿主机 slayerfs release 二进制（供镜像 COPY）"
bash "$DOCKER_DIR/build_slayerfs_host_binary.sh"

info "构建 xfstests runner 镜像"
docker compose -f "$COMPOSE_FILE" build xfstests

ts="$(date +%s)-$RANDOM"
export SLAYERFS_ARTIFACT_DIR="/artifacts/run-${ts}"
export XFSTESTS_CASES="$XFSTESTS_CASES_VALUE"
export XFSTESTS_CHECK_ARGS="$XFSTESTS_CHECK_ARGS_VALUE"
export XFSTESTS_SKIP_CASES="$XFSTESTS_SKIP_CASES_VALUE"
if [[ "$USE_S3" == true ]]; then
    export SLAYERFS_DATA_BACKEND="s3"
else
    export SLAYERFS_DATA_BACKEND="local-fs"
fi

info "启动依赖服务: redis + rustfs"
docker compose -f "$COMPOSE_FILE" up -d redis rustfs

info "初始化 rustfs bucket（一次性容器）"
docker compose -f "$COMPOSE_FILE" run --rm rustfs-init

info "运行 xfstests（退出码由 xfstests 容器决定）"
set +e
docker compose -f "$COMPOSE_FILE" run --rm xfstests
status=$?
set -e

ok "compose 运行结束 (exit=$status)"
ok "产物目录: $ARTIFACTS_DIR/run-${ts}"
exit "$status"
