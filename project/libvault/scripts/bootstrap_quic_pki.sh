#!/usr/bin/env bash
set -euo pipefail

# This script bootstraps a RustyVault instance for the QUIC PKI workflow.
# It will:
#   1. Initialise the vault (if not already initialised) using docs/examples/quic_pki/01-init.json
#   2. Unseal the vault with the generated keys
#   3. Write control-plane & data-plane policies
#   4. Mount the PKI backend and generate/import a root CA
#   5. Create control-plane & data-plane issuance roles
#
# Requirements: curl, jq
# Environment variables:
#   VAULT_ADDR (required)
#   ROOT_TOKEN (optional; if absent the script will use the token saved during init)
# Output artefacts are written to ${BOOTSTRAP_OUTPUT_DIR:-./bootstrap_artifacts}

if ! command -v jq >/dev/null; then
  echo "[ERROR] jq is required" >&2
  exit 1
fi

if [[ -z "${VAULT_ADDR:-}" ]]; then
  echo "[ERROR] VAULT_ADDR must be set (e.g. http://127.0.0.1:8200)" >&2
  exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
EXAMPLES_DIR="${QUIC_EXAMPLES_DIR:-$REPO_ROOT/docs/examples/quic_pki}"
if [[ ! -d "$EXAMPLES_DIR" ]]; then
  EXAMPLES_DIR="/opt/rusty_vault/examples"
fi
OUT_DIR="${BOOTSTRAP_OUTPUT_DIR:-$REPO_ROOT/bootstrap_artifacts}"
mkdir -p "$OUT_DIR"

AUTH_HEADER="X-Vault-Token"

health_json=$(curl -sS "$VAULT_ADDR/v1/sys/health" || echo '{}')
initialized=$(echo "$health_json" | jq -r 'try .initialized // false')
sealed=$(echo "$health_json" | jq -r 'try .sealed // true')

ROOT_TOKEN_FILE="$OUT_DIR/root-token.txt"
UNSEAL_KEYS_FILE="$OUT_DIR/unseal-keys.json"

if [[ "$initialized" != "true" ]]; then
  echo "[INFO] Initialising vault..."
  init_status=$(curl -sS -o "$OUT_DIR/init-response.json" -w '%{http_code}' \
    -X POST "$VAULT_ADDR/v1/sys/init" \
    -H 'Content-Type: application/json' \
    --data @"$EXAMPLES_DIR/01-init.json")

  if [[ "$init_status" -ge 300 ]]; then
    echo "[ERROR] Vault init failed (HTTP $init_status):" >&2
    cat "$OUT_DIR/init-response.json" >&2
    exit 1
  fi

  ROOT_TOKEN_LOCAL=$(jq -r '.root_token // empty' "$OUT_DIR/init-response.json")
  if [[ -z "$ROOT_TOKEN_LOCAL" ]]; then
    echo "[ERROR] Init response missing root_token" >&2
    exit 1
  fi
  printf '%s\n' "$ROOT_TOKEN_LOCAL" >"$ROOT_TOKEN_FILE"

  jq '.keys_base64 // .keys // empty' "$OUT_DIR/init-response.json" >"$UNSEAL_KEYS_FILE"
  if ! jq -e 'type == "array" and length > 0' "$UNSEAL_KEYS_FILE" >/dev/null 2>&1; then
    echo "[ERROR] Init response missing unseal keys" >&2
    cat "$OUT_DIR/init-response.json" >&2
    exit 1
  fi

  sealed=true
  initialized=true
  echo "[INFO] Init complete. Root token stored at $ROOT_TOKEN_FILE"
fi

if [[ "$sealed" == "true" ]]; then
  if [[ ! -s "$UNSEAL_KEYS_FILE" ]]; then
    echo "[ERROR] Vault is sealed and $UNSEAL_KEYS_FILE is missing or empty; provide keys manually." >&2
    exit 1
  fi

  echo "[INFO] Unsealing vault..."
  mapfile -t keys < <(jq -r '.[]' "$UNSEAL_KEYS_FILE")
  if [[ ${#keys[@]} -eq 0 ]]; then
    echo "[ERROR] No unseal keys found in $UNSEAL_KEYS_FILE" >&2
    exit 1
  fi

  threshold=$(jq -r 'try .secret_threshold // empty' "$OUT_DIR/init-response.json" 2>/dev/null || true)
  if [[ -z "$threshold" || "$threshold" == "null" ]]; then
    threshold=${#keys[@]}
  fi

  for ((i=0; i<threshold && i<${#keys[@]}; i++)); do
    key="${keys[$i]}"
    resp=$(curl -sS -X POST "$VAULT_ADDR/v1/sys/unseal" \
      -H 'Content-Type: application/json' \
      --data "{\"key\":\"$key\"}")
    sealed_now=$(echo "$resp" | jq -r '.sealed')
    echo "  share $((i+1)) applied (sealed=$sealed_now)"
    if [[ "$sealed_now" == "false" ]]; then
      break
    fi
  done
fi

declare ROOT_TOKEN_LOCAL="${ROOT_TOKEN:-}"
if [[ -z "$ROOT_TOKEN_LOCAL" ]]; then
  if [[ -f "$ROOT_TOKEN_FILE" ]]; then
    ROOT_TOKEN_LOCAL=$(<"$ROOT_TOKEN_FILE")
  else
    echo "[ERROR] ROOT_TOKEN not set and $ROOT_TOKEN_FILE missing." >&2
    exit 1
  fi
fi

vault_write_json() {
  local path=$1
  local json_file=$2
  curl -sS -X POST "$VAULT_ADDR/v1/$path" \
    -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" \
    -H 'Content-Type: application/json' \
    --data @"$json_file"
}

vault_write_raw() {
  local path=$1
  local body=$2
  curl -sS -X POST "$VAULT_ADDR/v1/$path" \
    -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" \
    -H 'Content-Type: application/json' \
    --data "$body"
}

echo "[INFO] Writing policies..."
for policy in control-plane data-plane; do
  hcl_path="$EXAMPLES_DIR/policy-$policy.hcl"
  curl -sS -X PUT "$VAULT_ADDR/v1/sys/policies/acl/$policy" \
    -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" \
    -H 'Content-Type: application/json' \
    --data @<(jq -Rs '{policy:.}' "$hcl_path") >/dev/null
  echo "  policy $policy loaded"
done

echo "[INFO] Ensuring PKI mount exists..."
mounts=$(curl -sS -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" "$VAULT_ADDR/v1/sys/mounts")
pki_present=$(echo "$mounts" | jq -e 'has("pki/")' >/dev/null && echo true || echo false)
if [[ "$pki_present" != "true" ]]; then
  curl -sS -X POST "$VAULT_ADDR/v1/sys/mounts/pki" \
    -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" \
    -H 'Content-Type: application/json' \
    --data @"$EXAMPLES_DIR/03-mount-pki.json" >/dev/null
  echo "  mounted pki/"
else
  echo "  pki/ already mounted"
fi

http_code=$(curl -sS -o /dev/null -w '%{http_code}' \
  -H "$AUTH_HEADER: $ROOT_TOKEN_LOCAL" \
  "$VAULT_ADDR/v1/pki/config/ca")
if [[ "$http_code" != "200" ]]; then
  echo "[INFO] Generating root CA in pki/ ..."
  resp=$(vault_write_json "pki/root/generate/exported" "$EXAMPLES_DIR/04-root-generate-exported.json")
  echo "$resp" | jq '.' >"$OUT_DIR/root-ca-response.json"
  echo "$resp" | jq -r '.data.certificate' >"$OUT_DIR/root-ca.pem"
  echo "$resp" | jq -r '.data.private_key' >"$OUT_DIR/root-ca-key.pem"
  echo "  root CA material saved under $OUT_DIR"
else
  echo "[INFO] Root CA already configured; skipping generation"
fi

echo "[INFO] Creating issuance roles..."
vault_write_json "pki/roles/control-plane" "$EXAMPLES_DIR/role-control-plane.json" >/dev/null
vault_write_json "pki/roles/data-plane" "$EXAMPLES_DIR/role-data-plane.json" >/dev/null

# Legacy quic-endpoint role retained for compatibility with earlier examples
vault_write_json "pki/roles/quic-endpoint" "$EXAMPLES_DIR/05-role-quic-endpoint.json" >/dev/null

echo "[SUCCESS] Vault bootstrap complete. Artefacts stored in $OUT_DIR"
echo "- Root token: $ROOT_TOKEN_FILE"
echo "- Unseal keys: $UNSEAL_KEYS_FILE"
