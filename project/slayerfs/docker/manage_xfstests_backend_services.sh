#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SLAYERFS_DIR="$(realpath "$SCRIPT_DIR/..")"

ACTION="${1:-}"
BACKEND="${2:-}"

usage() {
    cat <<EOF
用法: $(basename "$0") <up|down> <sqlite|redis|etcd>
EOF
    exit 0
}

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

if [[ -z "$ACTION" || -z "$BACKEND" ]]; then
    usage
fi

case "$ACTION" in
    up|down)
        ;;
    *)
        err "不支持的动作: $ACTION"
        usage
        ;;
esac

case "$BACKEND" in
    sqlite)
        COMPOSE_FILE="$SCRIPT_DIR/docker compose.sqlite.yml"
        SERVICES=()
        ;;
    redis)
        COMPOSE_FILE="$SCRIPT_DIR/docker compose.redis.yml"
        SERVICES=(redis)
        ;;
    etcd)
        COMPOSE_FILE="$SCRIPT_DIR/docker compose.etcd.yml"
        SERVICES=(etcd)
        ;;
    *)
        err "不支持的后端: $BACKEND"
        usage
        ;;
esac

if [[ ! -f "$COMPOSE_FILE" ]]; then
    err "找不到 compose 文件: $COMPOSE_FILE"
    exit 1
fi

require_compose() {
    if ! command -v docker >/dev/null 2>&1; then
        err "缺少必需命令: docker"
        exit 1
    fi
    if ! docker compose version >/dev/null 2>&1; then
        err "需要 docker compose (v2) 插件"
        exit 1
    fi
}

wait_for_service() {
    local service="$1"
    local deadline=$((SECONDS + 60))

    info "等待服务可用: $service"
    while (( SECONDS < deadline )); do
        if docker compose -f "$COMPOSE_FILE" exec -T "$service" true >/dev/null 2>&1; then
            ok "服务已就绪: $service"
            return 0
        fi
        sleep 2
    done

    err "等待服务超时: $service"
    exit 1
}

if [[ "${#SERVICES[@]}" -eq 0 ]]; then
    info "backend=$BACKEND 无需启动后端服务，compose 文件保留用于 slayerfs image 维护: $COMPOSE_FILE"
    exit 0
fi

require_compose

if [[ "$ACTION" == "up" ]]; then
    info "启动后端服务: ${SERVICES[*]}"
    docker compose -f "$COMPOSE_FILE" up -d "${SERVICES[@]}"
    for service in "${SERVICES[@]}"; do
        wait_for_service "$service"
    done
    ok "后端服务已启动"
else
    info "停止后端服务: ${SERVICES[*]}"
    docker compose -f "$COMPOSE_FILE" stop "${SERVICES[@]}" >/dev/null 2>&1 || true
    docker compose -f "$COMPOSE_FILE" rm -fsv "${SERVICES[@]}" >/dev/null 2>&1 || true
    ok "后端服务已停止"
fi
