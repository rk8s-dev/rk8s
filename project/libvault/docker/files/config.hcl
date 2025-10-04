storage "xline" {
    endpoints = ["http://127.0.0.1:2379"]
}

listener "tcp" {
    address     = "127.0.0.1:8200"
    tls_disable = "true"
    tls_cert_file = "servercert.pem"
    tls_key_file = "serverkey.pem"
    tls_disable_client_certs = false
    tls_require_and_verify_client_cert = false
}

daemon = true
daemon_user = "paul"
daemon_group = "staff"

work_dir = "/tmp/rusty_vault"

api_addr = "http://127.0.0.1:8200"
log_level = "debug"
pid_file = "rusty_vault.pid"

# -----------------------------------------------------------------------------
# Policy snippets to load with `vault policy write` (or equivalent CLI)
# -----------------------------------------------------------------------------

# control-plane.hcl
# path "pki/issue/control-plane" {
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

# data-plane.hcl
# path "pki/issue/data-plane" {
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
