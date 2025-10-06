# QUIC PKI Workflow Requests

This directory contains ready-to-use request bodies and a `curl` script that drives the full
lifecycle for using `libvault` as a root CA for QUIC modules. Replace placeholders before running.

## Files

- `01-init.json` – payload for `POST /v1/sys/init` to initialise the vault.
- `02-unseal.json` – template for `POST /v1/sys/unseal`; update `key` with each unseal share.
- `03-mount-pki.json` – body for `POST /v1/sys/mounts/pki` to mount the PKI backend.
- `04-root-generate-exported.json` – parameters for `POST /v1/pki/root/generate/exported`.
- `05-role-quic-endpoint.json` – legacy role definition for generic QUIC endpoints.
- `role-rks-node.json` – issuance parameters for control-plane (RKS node) certificates.
- `role-rkl-node.json` – issuance parameters for data-plane (RKL node) certificates.
- `06-issue-quic-server.json` – sample server certificate request.
- `07-enable-cert-auth.json` – body for `POST /v1/sys/auth/cert` to enable the cert auth backend.
- `08-auth-cert-entry.json` – template for registering a trusted CA at `POST /v1/auth/cert/certs/quic-clients`.
- `09-issue-quic-client.json` – sample client certificate request.
- `10-auth-cert-login.json` – body for `POST /v1/auth/cert/login`.
- `curl_requests.sh` – ordered sequence of the `curl` invocations above.
- `policy-rks-node.hcl`, `policy-rkl-node.hcl` – ACL policies to load into Vault.
- `docker/bootstrap_quic_pki.sh` – optional helper script that automates the steps above.
- `workflow_data_plane.md` – end-to-end example for data-plane nodes using bootstrap tokens.

### Token helpers in the Docker image

- `/usr/local/bin/create_rkl_bootstrap_token.sh` – issues a 30m/3-use bootstrap token bound to `rkl-node`; honours `VAULT_ADDR` (default `http://127.0.0.1:8200`) and falls back to `/opt/rusty_vault/state/root-token.txt` when `ROOT_TOKEN` is unset. Override behaviour with `TOKEN_TTL`, `TOKEN_MAX_TTL`, `TOKEN_USES`, `TOKEN_TYPE`, `TOKEN_DISPLAY_NAME`, `TOKEN_OUTPUT_PATH`, `ROOT_TOKEN_FILE`.
- `/usr/local/bin/create_rks_bootstrap_token.sh` – same behaviour for the `rks-node` policy.

## Usage Notes

1. Export `VAULT_ADDR` before running the script; after initialisation export `VAULT_TOKEN` from the
   `root_token` returned by `/v1/sys/init`.
2. Update placeholders:
   - `<base64-unseal-key>`: replace with each key share when unsealing.
   - `<paste-root-ca-pem-with-newlines-escaped>`: insert the PEM returned by the root generation step,
     with newlines encoded as `\n` or by using `jq -Rs` as shown in previous examples.
   - `<serial-with-hyphen>`: replace with the issued certificate serial (colon replaced by hyphen) when
     fetching stored certs.
3. Steps that depend on TLS client certificates (cert auth login) should be executed against a
   RustyVault instance configured with mutual TLS enabled.

These snippets mirror the API flow discussed in the QUIC certificate management plan and are ready to
be adapted into automation scripts or integration tests.
