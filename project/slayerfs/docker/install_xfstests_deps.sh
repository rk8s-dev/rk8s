#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SLAYERFS_DIR="$(realpath "$SCRIPT_DIR/..")"
PROJECT_DIR="$(realpath "$SLAYERFS_DIR/..")"
REPO_DIR="$(realpath "$PROJECT_DIR/..")"

SKIP_SYSTEM_DEPS=false
SKIP_LFS=false

usage() {
    cat <<EOF
用法: $(basename "$0") [选项]

选项:
  --skip-system-deps    跳过 apt 系统依赖安装
  --skip-lfs            跳过 git lfs pull
  -h, --help            显示帮助
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

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-system-deps)
            SKIP_SYSTEM_DEPS=true
            ;;
        --skip-lfs)
            SKIP_LFS=true
            ;;
        -h|--help)
            usage
            ;;
        *)
            err "未知参数: $1"
            usage
            ;;
    esac
    shift
done

require_cmd git

if [[ "$SKIP_SYSTEM_DEPS" == false ]]; then
    info "安装 xfstests 本地运行所需系统依赖"
    sudo apt-get update -y
    sudo apt-get install -y --no-install-recommends \
        git-lfs \
        fuse3 \
        libfuse3-dev \
        pkg-config \
        libssl-dev \
        sqlite3 \
        protobuf-compiler \
        qemu-system-x86 \
        qemu-utils \
        libvirt-clients \
        guestfish \
        xorriso
    ok "系统依赖安装完成"
else
    info "跳过系统依赖安装"
fi

if [[ "$SKIP_LFS" == false ]]; then
    info "拉取 xfstests 所需的 Git LFS 资源"
    cd "$REPO_DIR"
    git lfs install --local
    git lfs pull --include="project/slayerfs/tests/scripts/xfstests-prebuilt/*.tar.gz,project/slayerfs/tests/scripts/fuse3-bundle/fusermount3"
    ok "Git LFS 资源已就绪"
else
    info "跳过 Git LFS 拉取"
fi
