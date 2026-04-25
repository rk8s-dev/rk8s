#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(realpath "$SCRIPT_DIR/../..")"
BIN_PATH="$PROJECT_DIR/target/release/slayerfs"

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

pick_strip_tool() {
    if command -v llvm-strip >/dev/null 2>&1; then
        echo llvm-strip
        return 0
    fi
    if command -v strip >/dev/null 2>&1; then
        echo strip
        return 0
    fi
    return 1
}

cd "$PROJECT_DIR"

info "在宿主机构建 slayerfs release 二进制"
cargo build --release -p slayerfs --bin slayerfs

if [[ ! -f "$BIN_PATH" ]]; then
    err "构建完成后未找到二进制: $BIN_PATH"
    exit 1
fi

before_size=$(stat -c%s "$BIN_PATH")

if strip_tool=$(pick_strip_tool); then
    info "去除符号表: $strip_tool"
    if ! "$strip_tool" --strip-debug --strip-unneeded "$BIN_PATH" 2>/dev/null; then
        if ! "$strip_tool" --strip-unneeded "$BIN_PATH" 2>/dev/null; then
            "$strip_tool" "$BIN_PATH"
        fi
    fi
else
    err "未找到 strip 工具，请安装 binutils 或 llvm"
    exit 1
fi

after_size=$(stat -c%s "$BIN_PATH")
chmod 755 "$BIN_PATH"

ok "宿主机二进制已就绪: $BIN_PATH"
info "二进制大小: ${before_size} -> ${after_size} 字节"
