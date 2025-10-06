path "pki/issue/rkl-node" {
  capabilities = ["update"]
}

path "pki/cert/*" {
  capabilities = ["read"]
}

path "auth/token/lookup-self" {
  capabilities = ["read"]
}

path "auth/token/renew-self" {
  capabilities = ["update"]
}
