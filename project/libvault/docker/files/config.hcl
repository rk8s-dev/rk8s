storage "xline" {
    endpoints = ["http://etcd:2379"]
}

listener "tcp" {
    address     = "0.0.0.0:8200"
    tls_disable = "true"
    tls_cert_file = "servercert.pem"
    tls_key_file = "serverkey.pem"
    tls_disable_client_certs = false
    tls_require_and_verify_client_cert = false
}

daemon = false
daemon_user = "paul"
daemon_group = "staff"

work_dir = "/tmp/rusty_vault"

api_addr = "http://127.0.0.1:8200"
log_level = "debug"
pid_file = "rusty_vault.pid"

# -----------------------------------------------------------------------------
# Policy snippets to load with `vault policy write` (or equivalent CLI)
# -----------------------------------------------------------------------------

# policy-rks-node.hcl
# path "pki/issue/rks-node" {
#   capabilities = ["update"]
# }
#
# path "pki/cert/*" {
#   capabilities = ["read"]
# }
#
# path "pki/revoke" {
#   capabilities = ["update"]
# }
#
# path "auth/token/lookup-self" {
#   capabilities = ["read"]
# }
#
# path "auth/token/renew-self" {
#   capabilities = ["update"]
# }

# policy-rkl-node.hcl
# path "pki/issue/rkl-node" {
#   capabilities = ["update"]
# }
#
# path "pki/cert/*" {
#   capabilities = ["read"]
# }
#
# path "auth/token/lookup-self" {
#   capabilities = ["read"]
# }
#
# path "auth/token/renew-self" {
#   capabilities = ["update"]
# }
