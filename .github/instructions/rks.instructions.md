---
applyTo: "rks/**"
---

# RKS (Control Plane) – Path-specific Copilot Instructions

## Scope
RKS provides the **scheduler, state management, and reconciliation loops**, aligning with Kubernetes control-plane patterns.  
Priorities: **consistency, recoverability, observability**.

## Coding & design
- Implement controllers as explicit state machines (`Pending → Running → Succeeded/Failed`) — idempotent and re-entrant.  
- Storage/state backend should be pluggable with WAL/snapshot or event replay.  
- Scheduler: start simple (binpack/spread), keep extensible for affinity, taints, and priorities.  
- API layer (HTTP/gRPC): versioned, supports `watch`/streaming; avoid leaking internal structs.  
- Trace critical decisions (node binding, preemption, rollback) via `tracing`.

## Tests & benchmarks
- Property tests for controller logic (repeated/async events).  
- Recovery tests after restart (snapshot or event replay).  
- Scheduler load tests: large batch submission latency and failure injection (resource exhaustion, node loss).

## Copilot guidance
- Suggest Rust code for controller skeletons, event types, and store traits.  
- Emphasize re-entrancy, idempotency, and clear separation of desired vs actual state.
