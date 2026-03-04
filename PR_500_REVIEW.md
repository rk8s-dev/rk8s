## PR Review: fix(rks): add network for rks without rkl

### Overview

This PR adds local network bootstrapping capability to the `rks` node so it can configure networking independently, without requiring an `rkl` (Rust Kubernetes Layer) instance. It also upgrades `netlink-packet-route` (0.22.0 → 0.25.1) and `rtnetlink` (0.16.0 → 0.18.1), refactors the `NetworkConfigMessage::SubnetConfig` variant to pass raw `subnet_env` strings instead of pre-parsed structs, and removes the `qlean` dependency from `libvault`.

---

### Positive Aspects

1. **Clear separation of concerns**: The new `local_node.rs` module is well-structured with distinct functions for bootstrapping, session loop handling, lease completion, subnet env application, and route management.
2. **Simplification of `NetworkConfigMessage`**: Changing `SubnetConfig` from a multi-field struct to a single `subnet_env: String` reduces coupling — the sender no longer needs to understand the config structure, and parsing is deferred to the receiver.
3. **Buffer size increase** in `client.rs` (`4096 → 8192`): Addresses potential truncation issues with larger payloads from the new `SetNetwork` messages.
4. **Consistent logging**: Good use of structured `target` annotations (`rks::node::local`) throughout the new module.

---

### Issues & Suggestions

#### 1. 🔴 Stale routes are never cleaned up (Bug Risk)
`apply_routes()` only calls `add_route` / `add_v6_route` but **never removes routes** that may have been previously added. When a worker's lease is deleted or its subnet changes, old routes will remain in the kernel routing table, potentially leading to traffic blackholes or routing conflicts.

**Suggestion**: Maintain a set of currently-applied routes and perform a diff (add new routes, delete stale ones) on each `UpdateRoutes` message, or call a `flush` / `remove_route` before applying the new set.

#### 2. 🟡 `start_background_tasks` became blocking (Potential Startup Delay)
In `mod.rs`, `start_background_tasks` was changed from `fn` to `async fn`, and `local_node::bootstrap(...)` is **awaited** synchronously:
```rust
if let Err(e) = local_node::bootstrap(self.shared.clone(), &self.addr).await {
    warn!("Failed to bootstrap local rks node networking: {e:#}");
}
```
This means the QUIC server (`server.serve(...)`) won't start until the entire bootstrap completes — including `acquire_lease`, network discovery, etc. If any of these operations are slow or the network is unreachable, the rks node will be delayed in accepting connections.

**Suggestion**: Consider spawning the bootstrap as a background task (`tokio::spawn`) so it doesn't block server startup, or at least add a timeout.

#### 3. 🟡 Duplicated subnet-env parsing logic
The `subnet_env` string-parsing logic now exists in **three places**:
- `local_node.rs::apply_subnet_env` — parses `RKL_SUBNET`, `RKL_IPV6_SUBNET`, `RKL_MTU`, `RKL_IPMASQ`
- `receiver.rs::handle_network_config` — parses `RKL_NETWORK`, `RKL_SUBNET`, `RKL_MTU`, `RKL_IPMASQ`
- The old `client.rs::handle_network_config` (removed) — parsed the same fields

The two remaining parsers parse **different subsets of keys** (`RKL_IPV6_SUBNET` vs `RKL_NETWORK`) and construct different config types. This divergence is a potential source of bugs.

**Suggestion**: Extract a common `SubnetEnv` struct + parser (e.g., in `common`) that both sides can use to guarantee consistency.

#### 4. 🟡 IPv6 subnet not forwarded in receiver
In `receiver.rs`, after parsing the subnet env, IPv6 subnet is always passed as `None`:
```rust
self.subnet_receiver
    .handle_subnet_config(&network_config, ip_masq, subnet, None, mtu)
```
But the env string may contain `RKL_IPV6_SUBNET` — it's simply not parsed in `receiver.rs`. If IPv6 support is intended in rkl workers, this is a gap.

#### 5. 🟡 `resolve_local_node_id` relies on env vars without validation
The function falls through `RKS_NODE_NAME` → `HOSTNAME` → address-based suffix. The `HOSTNAME` path produces `rks-{hostname}` while the address fallback produces `rks-{sanitized_addr}`. This is fine, but there's no length limit on the resulting node ID, which could cause issues if the hostname is very long. Also, the `let ... && ...` syntax (`if let Ok(node_id) = ... && !node_id.trim().is_empty()`) requires `let_chains` — ensure `#![feature(let_chains)]` or MSRV ≥ 1.87.0.

#### 6. 🟢 Minor: `qlean` dependency removal from `libvault`
The PR removes `qlean/0.2.3` from `libvault/BUCK` and bumps `libvault` version to `0.2.2`. Make sure this removal is intentional and `qlean` is truly unused in `libvault`.

#### 7. 🟢 Minor: Windows `rustc_flags` added to `rkl/BUCK` and `rkforge/BUCK`
These appear unrelated to the stated PR goal ("add network for rks without rkl"). Consider splitting this into a separate PR or at least mentioning it in the PR description for clarity.

---

### Summary

| Category | Count |
|---|---|
| Bug Risk | 1 (stale route cleanup) |
| Design Concern | 3 (blocking bootstrap, duplicated parsing, IPv6 gap) |
| Minor / Nit | 3 (qlean removal, unrelated Windows flags, node ID length) |

The core feature — enabling rks to self-bootstrap networking without rkl — is well-implemented. The main concerns are: **(1)** stale route cleanup missing in `apply_routes`, and **(2)** the blocking bootstrap potentially delaying server startup. Addressing these two would significantly improve robustness.
