#!/usr/bin/env bash
set -euo pipefail

STATE_DIR="${BOOTSTRAP_OUTPUT_DIR:-/opt/rusty_vault/state}"
mkdir -p "$STATE_DIR/data"
chown -R rvault:rvault "$STATE_DIR"

VAULT_ADDR=${VAULT_ADDR:-http://127.0.0.1:8200}
export VAULT_ADDR

gosu rvault rvault server --config=/etc/rusty_vault/config.hcl &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID"
    wait "$SERVER_PID"
  fi
}
trap cleanup EXIT

# Wait for the HTTP listener to come up
for i in {1..60}; do
  if curl -sS "$VAULT_ADDR/v1/sys/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
  if [[ $i -eq 60 ]]; then
    echo "[ERROR] Vault did not become ready in time" >&2
    exit 1
  fi
done

BOOTSTRAP_OUTPUT_DIR="$STATE_DIR" ROOT_TOKEN="${ROOT_TOKEN:-}" gosu rvault /usr/local/bin/bootstrap_quic_pki.sh || true

echo "Vault running (PID $SERVER_PID). Logs will follow..."
wait "$SERVER_PID"
