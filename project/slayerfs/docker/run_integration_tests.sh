#!/usr/bin/env bash
# 用途:
#   在本地运行 slayerfs 集成测试，流程与 CI (slayerfs-tests.yml) 保持一致。
#   支持以下测试阶段：
#     1. 启动后端服务 (etcd / redis / postgres via docker compose)
#     2. 构建 persistence_demo 示例
#     3. 运行 qlean multinode smoke 集成测试
#     4. (可选) 运行 fuzz 探索 (--fuzz)
#     5. 清理后端服务

set -euo pipefail

# --------------------------------------------------------------------------- #
# 路径常量
# --------------------------------------------------------------------------- #
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SLAYERFS_DIR="$(realpath "$SCRIPT_DIR/..")"
PROJECT_DIR="$(realpath "$SLAYERFS_DIR/..")"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.integration.yml"

# --------------------------------------------------------------------------- #
# 命令行参数
# --------------------------------------------------------------------------- #
RUN_FUZZ=false
FUZZ_MAX_TIME=180       # 秒，与 CI 保持一致
FUZZ_RSS_LIMIT=1024     # MB
SKIP_DEPS=false
SKIP_SERVICES=false

usage() {
    cat <<EOF
用法: $(basename "$0") [选项]

  --fuzz              在集成测试后额外运行 fuzz 探索 (fs_ops target, ${FUZZ_MAX_TIME}s)
  --fuzz-time <秒>    自定义 fuzz 最大运行时间 (默认: ${FUZZ_MAX_TIME})
  --skip-deps         跳过系统依赖安装 (apt-get)
  --skip-services     跳过 docker compose 后端服务的启停 (适合已手动启动后端的场景)
  -h, --help          显示此帮助

示例:
  # 运行完整集成测试
  $SCRIPT_DIR/$(basename "$0")

  # 同时跑 fuzz
  $SCRIPT_DIR/$(basename "$0") --fuzz

  # 跳过依赖安装 + 已有后端服务时
  $SCRIPT_DIR/$(basename "$0") --skip-deps --skip-services
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --fuzz)          RUN_FUZZ=true ;;
        --fuzz-time)     FUZZ_MAX_TIME="$2"; shift ;;
        --skip-deps)     SKIP_DEPS=true ;;
        --skip-services) SKIP_SERVICES=true ;;
        -h|--help)       usage ;;
        *)
            echo "未知参数: $1" >&2
            usage
            ;;
    esac
    shift
done

# --------------------------------------------------------------------------- #
# 工具函数
# --------------------------------------------------------------------------- #
log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

check_cmd() {
    if ! command -v "$1" &>/dev/null; then
        err "缺少必需命令: $1"
        return 1
    fi
}

# --------------------------------------------------------------------------- #
# 1. 环境预检
# --------------------------------------------------------------------------- #
info "=== 环境预检 ==="
check_cmd cargo
check_cmd docker

if [[ "$SKIP_SERVICES" == false ]]; then
    if ! docker compose version &>/dev/null; then
        err "需要 docker compose (v2) 插件，请升级 Docker Desktop 或安装 docker compose-plugin"
        exit 1
    fi
fi

# --------------------------------------------------------------------------- #
# 2. 安装系统依赖
# --------------------------------------------------------------------------- #
if [[ "$SKIP_DEPS" == false ]]; then
    info "=== 安装系统依赖 ==="
    sudo apt-get update -y
    sudo apt-get install -y --no-install-recommends \
        fuse3 \
        pkg-config \
        libssl-dev \
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

# --------------------------------------------------------------------------- #
# 3. 启动后端服务
# --------------------------------------------------------------------------- #
cleanup_services() {
    if [[ "$SKIP_SERVICES" == false ]]; then
        info "停止后端服务..."
        docker compose -f "$COMPOSE_FILE" down -v || true
    fi
}

if [[ "$SKIP_SERVICES" == false ]]; then
    info "=== 启动后端服务 ==="
    docker compose -f "$COMPOSE_FILE" up -d
    ok "后端服务已启动 (etcd / redis / postgres)"

    # 注册退出时自动清理
    trap cleanup_services EXIT INT TERM
else
    info "跳过后端服务管理 (假设已在运行)"
    trap 'info "脚本结束"' EXIT INT TERM
fi

# --------------------------------------------------------------------------- #
# 4. 构建 persistence_demo
# --------------------------------------------------------------------------- #
info "=== 构建 persistence_demo ==="
cd "$PROJECT_DIR"
cargo build -p slayerfs --example persistence_demo --release
ok "persistence_demo 构建完成: target/release/examples/persistence_demo"

# --------------------------------------------------------------------------- #
# 5. 运行 qlean multinode smoke 集成测试
# --------------------------------------------------------------------------- #
info "=== 运行集成测试: test_slayerfs_qlean_multinode_smoke ==="
cd "$PROJECT_DIR"
cargo test -p slayerfs --test test_slayerfs_qlean_multinode_smoke -- --ignored
ok "集成测试通过"

# --------------------------------------------------------------------------- #
# 6. (可选) Fuzz 探索
# --------------------------------------------------------------------------- #
if [[ "$RUN_FUZZ" == true ]]; then
    info "=== Fuzz 探索: fs_ops (最长 ${FUZZ_MAX_TIME}s) ==="

    # 确保 nightly + cargo-fuzz 已安装
    if ! rustup toolchain list | grep -q nightly; then
        info "安装 nightly 工具链..."
        rustup toolchain install nightly --profile minimal
    fi
    if ! cargo +nightly fuzz --version &>/dev/null 2>&1; then
        info "安装 cargo-fuzz..."
        cargo install cargo-fuzz --locked --force
    fi

    cd "$SLAYERFS_DIR"
    cargo +nightly fuzz build fs_ops

    FUZZ_BIN=$(find fuzz/target -type f -path "*/release/fs_ops" | head -n 1)
    if [[ -z "$FUZZ_BIN" ]]; then
        err "找不到编译后的 fuzz binary"
        exit 1
    fi
    info "使用 fuzz binary: ${FUZZ_BIN}"

    mkdir -p fuzz/corpus/fs_ops fuzz/artifacts/fs_ops

    set +e
    "$FUZZ_BIN" \
        -artifact_prefix=fuzz/artifacts/fs_ops/ \
        fuzz/corpus/fs_ops \
        -rss_limit_mb="$FUZZ_RSS_LIMIT" \
        -max_total_time="$FUZZ_MAX_TIME" \
        -max_len=1024 \
        -print_final_stats=1
    FUZZ_STATUS=$?
    set -e

    if [[ "$FUZZ_STATUS" -eq 71 ]]; then
        ok "Fuzz 因 RSS 上限正常退出 (exit 71)"
    elif [[ "$FUZZ_STATUS" -ne 0 ]]; then
        err "Fuzz 以非零状态退出: $FUZZ_STATUS"
        err "产出物位于: $SLAYERFS_DIR/fuzz/artifacts/fs_ops/"
        exit "$FUZZ_STATUS"
    else
        ok "Fuzz 探索完成"
    fi

    # 清理临时 fuzz 产出
    rm -rf fuzz/artifacts/fs_ops/*
    rm -rf fuzz/.tmp*
    info "Fuzz 临时产出物已清理"
fi

# --------------------------------------------------------------------------- #
# 完成
# --------------------------------------------------------------------------- #
info "=== 全部测试完成 ==="
