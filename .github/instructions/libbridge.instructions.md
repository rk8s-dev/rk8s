---
applyTo: "libbridge/**"
---

# libbridge (CNI & IPAM) – Path-specific Copilot Instructions

## Scope
libbridge implements the **CNI bridge plugin** and **IPAM** for RK8s:  
managing network bridges, veth pairs, IP allocation, routing, and cleanup.

## Coding & design
- Split responsibilities: device management (netlink), address allocation, routing rules, and firewall/nftables setup.  
- IPAM persistence: maintain lease records (subnet, allocation time, owner) — must be thread-safe.  
- Cleanup and rollback on errors (veth removal, IP release, route revert).  
- Ensure CNI compatibility — expose minimal required fields and keep future plugin interoperability in mind.  
- Emit `tracing` events with interface name, bridge, subnet, operation, and duration.

## Tests
- Unit tests for IPAM logic and helper utilities.  
- Integration tests with network namespaces (`netns`, `ip`) — optional in CI.  
- Cover edge cases: IP exhaustion, existing interfaces, stale routes, or leftover rules.

## Copilot guidance
- Generate examples for: bridge creation, veth setup, IP allocation, route configuration, and rollback handling.  
- Recommend cross-platform safe Rust libraries (e.g., `netlink-packet`, `nftnl`, `ipnetwork`).
