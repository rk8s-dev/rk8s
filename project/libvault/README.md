# libvault – RustyVault Library

`libvault` is the **embedded Rust library** that provides the core functionality of
RustyVault, a lightweight, HashiCorp‑Vault‑compatible secrets manager written in
Rust.  It is used by RK8s as the certificate authority and general
key‑value store for the control plane (RKS) and data nodes (RKL).

## Table of Contents

1. [Why libvault?](#why-libvault)
2. [Key Concepts](#key-concepts)
3. [Architecture Overview](#architecture-overview)
4. [Public API (Rust)](#public-api-rust)
5. [Typical Workflows](#typical-workflows)
   - [Bootstrapping a New Cluster](#bootstrapping-a-new-cluster)
   - [Storing & Retrieving Secrets](#storing--retrieving-secrets)
   - [PKI / Certificate Issuance](#pki--certificate-issuance)
6. [Modules](#modules)
7. [Storage Back‑ends](#storage-back-ends)
8. [Security Model](#security-model)
9. [Building & Testing](#building--testing)
10. [License](#license)

---

## Why libvault?

* **Embedded** – No external process is required; you can embed the vault directly
  into any Rust binary (e.g., the RKS control plane).
* **Full Vault semantics** – Supports KV, PKI, auth methods, policies, and
  token‑based access control.
* **Zero‑knowledge storage** – All secret material is encrypted with a
  *Security Barrier* before being persisted.
* **Modern Rust** – Async‑ready, `ArcSwap`‑based lock‑free state, `zeroize`
  for secret memory wiping, and `thiserror` for typed errors.

---

## Key Concepts

| Concept | Description |
|---------|-------------|
| **Seal / Unseal** | The vault starts in a sealed state.  A *seal configuration* defines how many unseal key shares are required (`secret_shares`) and the threshold (`secret_threshold`).  Unsealing reconstructs the master KEK using Shamir's Secret Sharing. |
| **Root Token** | After a successful `init`, a root token is generated.  It grants full privileges and can be used for the first login. |
| **Mounts** | Logical back‑ends (e.g., `kv/`, `pki/`) are mounted under a path.  Mounts are stored in the encrypted barrier and can be added/removed at runtime. |
| **Modules** | Extensible components (`auth`, `kv`, `pki`, `credential`, `policy`, `crypto`, `system`) that implement the `Module` trait and are managed by `ModuleManager`. |
| **Handlers** | Low‑level request/response hooks (`pre_route`, `route`, `post_route`, `log`) that can be added to customise processing. |
| **Physical Backend** | The actual storage implementation (file‑based, Xline KV, etc.) that the barrier reads/writes to. |

---

## Architecture Overview

```
+-------------------+      +-------------------+      +-------------------+
|   RustyVault      |      |   SecurityBarrier |      |   PhysicalBackend |
|   (Core)          |<---->|   (AES‑GCM)       |<---->|   (File / Xline) |
+-------------------+      +-------------------+      +-------------------+
        ^   ^                         ^                     ^
        |   |                         |                     |
        |   |                 +-------+-------+   +--------+--------+
        |   +---------------->|   MountsRouter|   |   ModuleManager |
        |                     +---------------+   +-----------------+
        |                           |
        |                +----------+----------+
        |                |   Logical Back‑ends |
        |                +---------------------+
        |                (KV, PKI, Credential …)
        +--------------------+-------------------+
```

* **Core** – Holds the runtime state (`CoreState`), the barrier, router, and module manager.
* **SecurityBarrier** – Encrypts/decrypts all data using a KEK derived from unseal keys.
* **PhysicalBackend** – Pluggable storage.  The default is a mock backend for tests; production uses a file backend (during bootstrap) or Xline KV after migration.
* **MountsRouter** – Resolves a request path (`sys/mounts/...`) to the appropriate logical backend.
* **Modules** – Provide higher‑level functionality (e.g., PKI creates certificates, KV stores secrets).

---

## Public API (Rust)

`libvault` exposes a single entry point: `RustyVault`.

```rust
use libvault::{RustyVault, Config, core::SealConfig, storage};
use std::collections::HashMap;

// 1️⃣ Choose a physical backend (file‑based for bootstrap)
let mut conf = HashMap::new();
conf.insert("path".to_string(), serde_json::json!("/var/lib/rks/vault"));
let backend = storage::new_backend("file", &conf)?;

// 2️⃣ Optional configuration (mount HMAC level, monitor interval, …)
let config = Some(&Config::default());

// 3️⃣ Create the vault instance
let vault = RustyVault::new(backend, config)?;

// 4️⃣ Initialise the vault (first time only)
let seal_cfg = SealConfig { secret_shares: 5, secret_threshold: 3 };
let init_res = vault.init(&seal_cfg).await?;
println!("Root token: {}", init_res.root_token);

// 5️⃣ Unseal with enough key shares
let unseal_keys = vec![ /* share 1 */, /* share 2 */, /* share 3 */ ];
for key in unseal_keys {
    if vault.unseal_once(key).await?.is_some() {
        break;
    }
}
// Note: `unseal_once` rotates keys automatically upon successful unseal

// 6️⃣ Set the root token for subsequent calls
vault.set_token(init_res.root_token);

// 7️⃣ Example: mount a KV engine and store a secret
vault.mount(None, "secret", "kv").await?;
let data = serde_json::json!({ "value": "super‑secret" }).as_object().cloned();
vault.write(None, "secret/data/mykey", data).await?;
let resp = vault.read(None, "secret/data/mykey").await?;
println!("Secret: {:?}", resp);
```

### Core Methods

| Method | Purpose |
|--------|---------|
| `new(backend, config)` | Create a new `RustyVault` instance. |
| `init(seal_cfg)` | Initialise the vault, generate root token and (optionally) initial unseal keys. |
| `unseal(key)` / `unseal_once(key)` | Provide unseal key(s); `unseal_once` also rotates keys. |
| `seal()` | Seal the vault, wiping KEK from memory. |
| `mount(token, path, type)` | Create a new logical mount (e.g., `kv`, `pki`). |
| `unmount(token, path)` | Remove a mount. |
| `read/write/delete/list(token, path, data)` | Low‑level CRUD operations on logical back‑ends. |
| `login(path, data)` | Perform an auth login; stores returned client token for later calls. |
| `set_token(token)` | Manually set the client token to be used for subsequent calls. |

All async methods return `Result<_, libvault::errors::RvError>`.

---

## Typical Workflows

### Bootstrapping a New Cluster

1. **Generate a temporary PKI** – `rks gen pems --config config.yaml` starts an embedded
   Vault with a **file backend** and creates a root CA, RKS certificate, and an Xline
   KV certificate.
2. **Persist PEM files** – The generated PEMs are saved to disk for later use.
3. **Start RKS** – `rks start --config config.yaml` launches the control plane,
   unseals the embedded Vault and migrates data from the file backend to Xline.
4. **Migration** – Vault reads all encrypted entries from the file backend and
   writes them to Xline, after which Xline becomes the primary storage.

> The above steps are captured in the original Mermaid diagrams; the Rust
> implementation follows the same sequence via the `RustyVault` API.

### Storing & Retrieving Secrets

```rust
// Mount a KV engine at `secret/`
vault.mount(None, "secret", "kv").await?;

// Write a secret
let data = serde_json::json!({
    "api_key": "abcd‑1234‑efgh‑5678"
}).as_object().cloned();
vault.write(None, "secret/data/app/config", data).await?;

// Read it back
let resp = vault.read(None, "secret/data/app/config").await?;
println!("Config: {:?}", resp);
```

### PKI / Certificate Issuance

```rust
// Enable the PKI engine
vault.mount(None, "pki", "pki").await?;

// Configure a role that allows issuing leaf certificates
let role_data = serde_json::json!({
    "allowed_domains": "example.com",
    "allow_subdomains": true,
    "max_ttl": "72h"
});
vault.write(None, "pki/roles/example", Some(role_data.as_object().unwrap().clone())).await?;

// Issue a leaf certificate (CSR is sent in the request body)
let csr = std::fs::read_to_string("node.csr")?;
let issue_data = serde_json::json!({ "csr": csr, "common_name": "node.example.com" });
let cert_resp = vault.write(None, "pki/issue/example", Some(issue_data.as_object().unwrap().clone())).await?;
println!("Certificate: {:?}", cert_resp);
```

---

## Modules

| Module | Responsibility |
|--------|----------------|
| **auth** | Token issuance, login, token revocation, token lookup. |
| **kv** | Simple key‑value store (versioned, metadata, TTL). |
| **pki** | Certificate Authority, role‑based issuance, CRL generation. |
| **credential / cert** | Higher‑level certificate handling used by RK8s for node bootstrapping. |
| **policy** | ACL policies that control token capabilities. |
| **crypto** | Cryptographic helpers (e.g., HMAC, RSA/ECDSA adapters). |
| **system** | Internal system paths (`sys/`) such as seal config, health checks, and mounts. |

Modules are loaded automatically by `RustyVault::new`; additional custom modules can be added via
`Core::add_handler` or `Core::add_auth_handler`.

---

## Storage Back‑ends

* **FileBackend** – Plain files under a directory (used for the initial bootstrap).  
  Data is encrypted with the barrier before being written.
* **XlineBackend** – Distributed KV store (used in production).  
  The migration path `Core::migrate` copies all entries from the file backend to Xline.
* **MockBackend** – In‑memory backend used by unit tests.

Backends implement the `storage::Backend` trait and can be swapped at runtime.

---

## Security Model

* **Seal / Unseal** – Vault starts sealed.  Unseal requires a threshold of key shares
  generated via Shamir’s Secret Sharing.  Keys are held in a `Zeroizing<Vec<u8>>`
  wrapper to guarantee memory wiping.
* **Zero‑knowledge** – All secret material (keys, certificates, KV entries) is stored
  behind the **AES‑GCM barrier**.  The barrier key (KEK) is never persisted.
* **Token‑based auth** – Every request must include a client token; tokens carry
  policies that restrict capabilities.
* **In‑memory only for private keys** – When RKL nodes receive a client certificate,
  the private key is never written to disk; it lives only in process memory.
* **Audit logging** – Structured audit events for each request phase are emitted via
  `Handler::log` on top of the `tracing` crate, while some components still emit
  traditional logs via the `log` crate. Configure observability to handle both.

---

## Building & Testing

```sh
# Build the library
cargo build -p libvault

# Run unit tests (including async tests)
cargo test -p libvault

# Benchmark hot paths (e.g., seal/unseal, KV read/write)
cargo bench -p libvault
```

The crate follows the repository‑wide policies:

* `rustfmt` with defaults.
* `clippy` warnings are treated as errors for new code.
* Avoid `unwrap()` / `expect()` in new library code and public APIs; propagate errors as `RvError` instead.

---

## License

`libvault` is dual‑licensed under the **Apache‑2.0** and **MIT** licenses, as
declared in the `LICENSE-APACHE` and `LICENSE-MIT` files at the repository root.

--- 

*For further details, see the source code under `src/` and the
`copilot-instructions.md` file for repository‑wide conventions.*
