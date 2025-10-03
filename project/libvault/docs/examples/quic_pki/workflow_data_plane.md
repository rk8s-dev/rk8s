# Data Plane Bootstrap Workflow (Token ➜ Certificate)

This walkthrough assumes you have already run `scripts/bootstrap_quic_pki.sh` (or otherwise
completed the setup described in this directory) so that the following artefacts exist:

- Policies `control-plane` and `data-plane` have been written to Vault.
- `pki/` mount is available with a configured root/intermediate CA.
- Role `data-plane` (from `role-data-plane.json`) exists under `/v1/pki/roles/data-plane`.

Below is an end-to-end example showing how an administrator issues a short-lived token for a data
plane node, and how that node exchanges it for a certificate.

## 1. Administrator creates a limited-use token

Use the root (or management) token to mint a child token tied to the `data-plane` policy. The token
is limited to three uses and has a 30-minute TTL. Adjust according to your security requirements.

```bash
cat <<'JSON' > /tmp/data-plane-token.json
{
  "policies": ["data-plane"],
  "display_name": "quic-data-bootstrap",
  "ttl": "30m",
  "explicit_max_ttl": "2h",
  "num_uses": 3,
  "token_type": "batch"
}
JSON

curl -sS -X POST "$VAULT_ADDR/v1/auth/token/create" \
  -H "X-Auth-Token: $ROOT_TOKEN" \
  -H "Content-Type: application/json" \
  --data @/tmp/data-plane-token.json | jq '.auth.client_token' -r > /tmp/data-plane.token
```

Deliver `/tmp/data-plane.token` to the node through a secure channel. Using `token_type=batch` keeps
the token out of persistent storage on the Vault side; wrapping tokens (`X-Vault-Wrap-TTL`) can add an
extra layer of protection in transit.

## 2. Node checks the token (optional)

The node can query Vault to inspect its own token metadata.

```bash
DATA_TOKEN=$(cat /tmp/data-plane.token)

curl -sS -X POST "$VAULT_ADDR/v1/auth/token/lookup" \
  -H "X-Auth-Token: $ROOT_TOKEN" \
  -H "Content-Type: application/json" \
  --data "{\"token\":\"$DATA_TOKEN\"}"
```

(This step requires management privileges. The node itself should use
`/v1/auth/token/lookup-self` with its token.)

## 3. Node requests a certificate

Use the token as the `X-Auth-Token` header when calling `/v1/pki/issue/data-plane`. Provide the
subject information via JSON body (for example, the contents of
`docs/examples/quic_pki/role-data-plane.json` determine what SAN/CN values are permitted).

```bash
cat <<'JSON' > /tmp/data-plane-cert-request.json
{
  "common_name": "node-01.data.svc.cluster.local",
  "alt_names": "node-01.data.svc.cluster.local,node-01",
  "ip_sans": "10.20.0.11",
  "ttl": "10h"
}
JSON

curl -sS -X POST "$VAULT_ADDR/v1/pki/issue/data-plane" \
  -H "X-Auth-Token: $DATA_TOKEN" \
  -H "Content-Type: application/json" \
  --data @/tmp/data-plane-cert-request.json | tee /tmp/data-plane-cert.json

jq -r '.data.certificate'    /tmp/data-plane-cert.json > node-01-cert.pem
jq -r '.data.private_key'    /tmp/data-plane-cert.json > node-01-key.pem
jq -r '.data.issuing_ca'     /tmp/data-plane-cert.json > node-01-issuing-ca.pem
jq -r '.data.ca_chain'       /tmp/data-plane-cert.json > node-01-ca-chain.pem
```

Install `node-01-cert.pem` and `node-01-key.pem` on the data-plane node, along with the CA chain as
needed by your QUIC/TLS stack.

## 4. Token cleanup / renewal

Because the token had `num_uses=3`, each call to Vault decreases the remaining uses. You can inspect
remaining uses via `auth/token/lookup-self`. After the final use, the token is automatically revoked.

If the node needs to request another certificate later, either:

1. Keep the token renewable by setting `token_period` and regularly calling `/v1/auth/token/renew-self`, or
2. Have the administrator mint a new bootstrap token, or
3. Switch to certificate-based auth (`auth/cert/login`) before the certificate expires.

## 5. Optional: revoke the old certificate (once implemented)

When you replace an existing certificate, remember to call `POST /v1/pki/revoke` with the old
serial number (this endpoint currently returns `NotImplemented`, so additional work is required in
`src/modules/pki/path_revoke.rs`).

---

This workflow matches the policies and role definitions provided in this directory, keeping the data
plane restricted to `pki/issue/data-plane` and read-only access to issued certificates.
