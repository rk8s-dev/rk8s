#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SLAYERFS_DIR="$(realpath "$SCRIPT_DIR/..")"
PROJECT_DIR="$(realpath "$SLAYERFS_DIR/..")"
INSTALL_DEPS_SCRIPT="$SCRIPT_DIR/install_xfstests_deps.sh"
MANAGE_SERVICES_SCRIPT="$SCRIPT_DIR/manage_xfstests_backend_services.sh"
EXCLUDE_FILE="$SLAYERFS_DIR/tests/scripts/xfstests_slayer.exclude"

BACKEND="${1:-}"
if [[ -z "$BACKEND" ]]; then
    echo "用法: $(basename "$0") <sqlite|redis|etcd> [选项]" >&2
    exit 1
fi
shift || true

case "$BACKEND" in
    sqlite)
        TEST_NAME="test_slayerfs_kvm_xfstests_sqlite"
        REQUIRED_SERVICES=()
        ;;
    redis)
        TEST_NAME="test_slayerfs_kvm_xfstests_redis"
        REQUIRED_SERVICES=(redis)
        ;;
    etcd)
        TEST_NAME="test_slayerfs_kvm_xfstests_etcd"
        REQUIRED_SERVICES=(etcd)
        ;;
    *)
        echo "不支持的后端: $BACKEND" >&2
        echo "可选值: sqlite, redis, etcd" >&2
        exit 1
        ;;
esac

SKIP_DEPS=false
SKIP_LFS=false
SKIP_BUILD=false
SKIP_SERVICES=false
KEEP_SERVICES=false
XFSTESTS_TIMEOUT_SECS_VALUE=""
XFSTESTS_FORCE_RECLONE_VALUE=""
ARTIFACT_ROOT_VALUE=""

usage() {
    cat <<EOF
用法: $(basename "$0") <sqlite|redis|etcd> [选项]

选项:
  --skip-deps                 跳过 apt 系统依赖安装
  --skip-lfs                  跳过 git lfs pull
  --skip-build                跳过 persistence_demo 构建
  --skip-services             跳过 docker compose 服务启停
  --keep-services             结束时不关闭已启动服务
  --timeout-secs <秒>         覆盖 SLAYERFS_XFSTESTS_TIMEOUT_SECS
  --force-reclone <0|1>       覆盖 SLAYERFS_XFSTESTS_FORCE_RECLONE
  --artifact-root <目录>      覆盖 SLAYERFS_XFSTESTS_HOST_ARTIFACT_ROOT
  -h, --help                  显示帮助

示例:
  $SCRIPT_DIR/$(basename "$0") sqlite
    $SCRIPT_DIR/$(basename "$0") redis
  $SCRIPT_DIR/$(basename "$0") etcd --skip-deps --timeout-secs 14400
EOF
    exit 0
}

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "缺少必需命令: $1"
        exit 1
    fi
}

require_value() {
    local option="$1"
    local value="${2:-}"
    if [[ -z "$value" ]]; then
        err "$option 需要提供参数值"
        exit 1
    fi
}

cleanup_services() {
    if [[ "$SKIP_SERVICES" == true || "$KEEP_SERVICES" == true || "${#REQUIRED_SERVICES[@]}" -eq 0 ]]; then
        return 0
    fi

    "$MANAGE_SERVICES_SCRIPT" down "$BACKEND"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-deps)
            SKIP_DEPS=true
            ;;
        --skip-lfs)
            SKIP_LFS=true
            ;;
        --skip-build)
            SKIP_BUILD=true
            ;;
        --skip-services)
            SKIP_SERVICES=true
            ;;
        --keep-services)
            KEEP_SERVICES=true
            ;;
        --timeout-secs)
            require_value "$1" "${2:-}"
            XFSTESTS_TIMEOUT_SECS_VALUE="$2"
            shift
            ;;
        --force-reclone)
            require_value "$1" "${2:-}"
            XFSTESTS_FORCE_RECLONE_VALUE="$2"
            shift
            ;;
        --artifact-root)
            require_value "$1" "${2:-}"
            ARTIFACT_ROOT_VALUE="$2"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "未知参数: $1" >&2
            usage
            ;;
    esac
    shift
done

trap cleanup_services EXIT INT TERM

info "=== SlayerFS xfstests 本地运行: backend=$BACKEND ==="

require_cmd cargo
require_cmd docker
require_cmd git
if [[ ! -x "$INSTALL_DEPS_SCRIPT" ]]; then
    err "依赖准备脚本不可执行: $INSTALL_DEPS_SCRIPT"
    exit 1
fi
if [[ ! -x "$MANAGE_SERVICES_SCRIPT" ]]; then
    err "服务管理脚本不可执行: $MANAGE_SERVICES_SCRIPT"
    exit 1
fi

if [[ ! -f "$EXCLUDE_FILE" ]]; then
    err "找不到 xfstests exclude 文件: $EXCLUDE_FILE"
    exit 1
fi

info "默认使用 exclude 文件: $EXCLUDE_FILE"

DEPS_ARGS=()
if [[ "$SKIP_DEPS" == true ]]; then
    DEPS_ARGS+=(--skip-system-deps)
fi
if [[ "$SKIP_LFS" == true ]]; then
    DEPS_ARGS+=(--skip-lfs)
fi
"$INSTALL_DEPS_SCRIPT" "${DEPS_ARGS[@]}"

if [[ "$SKIP_SERVICES" == false && "${#REQUIRED_SERVICES[@]}" -gt 0 ]]; then
    "$MANAGE_SERVICES_SCRIPT" up "$BACKEND"
else
    info "跳过后端服务管理"
fi

if [[ "$SKIP_BUILD" == false ]]; then
    info "构建 persistence_demo，与 CI 的 xfstests job 保持一致"
    cd "$PROJECT_DIR"
    cargo build -p slayerfs --example persistence_demo --release
    ok "persistence_demo 构建完成"
else
    info "跳过 persistence_demo 构建"
fi

ARTIFACT_ROOT="${ARTIFACT_ROOT_VALUE:-/tmp/slayerfs-kvm-xfstests/local/$BACKEND}"
mkdir -p "$ARTIFACT_ROOT"

info "运行 ignored xfstests 测试: $TEST_NAME"
cd "$PROJECT_DIR"

ENV_ARGS=("SLAYERFS_XFSTESTS_HOST_ARTIFACT_ROOT=$ARTIFACT_ROOT")
if [[ -n "$XFSTESTS_TIMEOUT_SECS_VALUE" ]]; then
    ENV_ARGS+=("SLAYERFS_XFSTESTS_TIMEOUT_SECS=$XFSTESTS_TIMEOUT_SECS_VALUE")
fi
if [[ -n "$XFSTESTS_FORCE_RECLONE_VALUE" ]]; then
    ENV_ARGS+=("SLAYERFS_XFSTESTS_FORCE_RECLONE=$XFSTESTS_FORCE_RECLONE_VALUE")
fi

env "${ENV_ARGS[@]}" \
    cargo test -p slayerfs --test test_slayerfs_kvm_integration \
    "$TEST_NAME" -- --ignored --nocapture

ok "xfstests 本地运行完成，产物根目录: $ARTIFACT_ROOT"
