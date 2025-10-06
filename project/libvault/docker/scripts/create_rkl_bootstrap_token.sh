#!/usr/bin/env bash
set -euo pipefail

# Generates a bootstrap token for RKL nodes with sane defaults.


VAULT_ADDR=${VAULT_ADDR:-http://127.0.0.1:8200}
if [[ -z "$VAULT_ADDR" ]]; then
  echo "[ERROR] VAULT_ADDR must be set" >&2
  exit 1
fi

ROOT_TOKEN=${ROOT_TOKEN:-}
ROOT_TOKEN_FILE=${ROOT_TOKEN_FILE:-/opt/rusty_vault/state/root-token.txt}
if [[ -z "$ROOT_TOKEN" && -f "$ROOT_TOKEN_FILE" ]]; then
  ROOT_TOKEN=$(<"$ROOT_TOKEN_FILE")
fi

if [[ -z "$ROOT_TOKEN" ]]; then
  echo "[ERROR] ROOT_TOKEN must be set or $ROOT_TOKEN_FILE must exist" >&2
  exit 1
fi

TTL=${TOKEN_TTL:-30m}
MAX_TTL=${TOKEN_MAX_TTL:-2h}
USES=${TOKEN_USES:-3}
TOKEN_TYPE=${TOKEN_TYPE:-batch}
DISPLAY_NAME=${TOKEN_DISPLAY_NAME:-quic-rkl-bootstrap}
OUTFILE=${TOKEN_OUTPUT_PATH:-}

payload=$(jq -n \
  --arg policy "rkl-node" \
  --arg display "$DISPLAY_NAME" \
  --arg ttl "$TTL" \
  --arg max_ttl "$MAX_TTL" \
  --arg uses "$USES" \
  --arg type "$TOKEN_TYPE" \
  '{policies: [$policy], display_name: $display, ttl: $ttl, explicit_max_ttl: $max_ttl, num_uses: ($uses | tonumber), token_type: $type}')

tmp_resp=$(mktemp)
http_code=$(curl -sS -o "$tmp_resp" -w '%{http_code}' \
  -X POST "$VAULT_ADDR/v1/auth/token/create" \
  -H "X-Vault-Token: $ROOT_TOKEN" \
  -H 'Content-Type: application/json' \
  --data "$payload")

if [[ "$http_code" -ge 300 ]]; then
  echo "[ERROR] Vault returned HTTP $http_code" >&2
  cat "$tmp_resp" >&2
  rm -f "$tmp_resp"
  exit 1
fi

token=$(jq -r '.auth.client_token // empty' "$tmp_resp")
if [[ -z "$token" ]]; then
  echo "[ERROR] Response did not include a client_token" >&2
  cat "$tmp_resp" >&2
  rm -f "$tmp_resp"
  exit 1
fi

wrapped=$(jq -r '.wrap_info.token // empty' "$tmp_resp")
rm -f "$tmp_resp"

if [[ -n "$OUTFILE" ]]; then
  printf '%s\n' "$token" >"$OUTFILE"
  echo "Bootstrap token written to $OUTFILE"
else
  printf '%s\n' "$token"
fi

if [[ -n "$wrapped" ]]; then
  echo "Wrapped token available: $wrapped" >&2
fi
