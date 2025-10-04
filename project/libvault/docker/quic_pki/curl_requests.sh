#!/usr/bin/env bash
set -euo pipefail

: "${VAULT_ADDR:?set VAULT_ADDR}" >/dev/null

echo "## 1. Initialize cluster"
curl -sS -X POST "$VAULT_ADDR/v1/sys/init" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/01-init.json"

echo "## 2. Check seal status"
curl -sS "$VAULT_ADDR/v1/sys/seal-status"

echo "## 3. Unseal (repeat with different keys)"
curl -sS -X POST "$VAULT_ADDR/v1/sys/unseal" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/02-unseal.json"

echo "## 4. Mount PKI backend"
curl -sS -X POST "$VAULT_ADDR/v1/sys/mounts/pki" \
  -H "X-Auth-Token: ${VAULT_TOKEN:?set VAULT_TOKEN}" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/03-mount-pki.json"

echo "## 5. Generate exported root CA"
curl -sS -X POST "$VAULT_ADDR/v1/pki/root/generate/exported" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/04-root-generate-exported.json"

echo "## 6. Create QUIC issuance role"
curl -sS -X POST "$VAULT_ADDR/v1/pki/roles/quic-endpoint" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/05-role-quic-endpoint.json"

echo "## 7. Issue server certificate"
curl -sS -X POST "$VAULT_ADDR/v1/pki/issue/quic-endpoint" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/06-issue-quic-server.json"

echo "## 8. Enable cert auth backend"
curl -sS -X POST "$VAULT_ADDR/v1/sys/auth/cert" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/07-enable-cert-auth.json"

echo "## 9. Register trusted CA for QUIC clients"
curl -sS -X POST "$VAULT_ADDR/v1/auth/cert/certs/quic-clients" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/08-auth-cert-entry.json"

echo "## 10. Issue client certificate"
curl -sS -X POST "$VAULT_ADDR/v1/pki/issue/quic-endpoint" \
  -H "X-Auth-Token: $VAULT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/09-issue-quic-client.json"

echo "## 11. Client cert login (requires TLS client cert)"
curl -sS -X POST "$VAULT_ADDR/v1/auth/cert/login" \
  -H "Content-Type: application/json" \
  --data @"$(dirname "$0")/10-auth-cert-login.json"

echo "## 12. Fetch stored certificate by serial (replace <serial>)"
curl -sS -H "X-Auth-Token: $VAULT_TOKEN" "$VAULT_ADDR/v1/pki/cert/<serial-with-hyphen>"
