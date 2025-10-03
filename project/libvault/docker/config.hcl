storage "xline" {
  endpoints = ["http://etcd:2379"]
}

listener "tcp" {
  address                        = "0.0.0.0:8200"
  tls_disable                    = "true"
  tls_disable_client_certs       = true
  tls_require_and_verify_client_cert = false
}

api_addr = "http://0.0.0.0:8200"
cluster_addr = "http://0.0.0.0:8201"
log_level = "info"
pid_file = "/opt/rusty_vault/state/rusty_vault.pid"
work_dir = "/opt/rusty_vault/state"
disable_mlock = true
