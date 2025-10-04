path "pki/issue/control-plane" {
  capabilities = ["update"]
}

path "pki/cert/*" {
  capabilities = ["read"]
}

path "pki/revoke" {
  capabilities = ["update"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}

path "auth/token/renew-self" {
  capabilities = ["update"]
}
