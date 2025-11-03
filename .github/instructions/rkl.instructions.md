---
applyTo: "rkl/**"
---

# RKL (Runtime) – Path-specific Copilot Instructions

## Scope
RKL implements the **container and pod runtime**:
- Pod lifecycle (create, start, stop, cleanup)
- Namespace, cgroup, and filesystem setup
- OCI/CRI interoperability

Goals: **fast startup**, **low overhead**, **robust observability**, suitable for both edge and developer machines.

## Coding & design
- Separate responsibilities: image fetch, rootfs setup, namespace/mount, cgroup, exec/attach.  
- Use non-blocking async I/O (Tokio); long-running tasks must support cancellation and timeouts.  
- Emit `tracing` spans for all lifecycle events (`container_id`, `pod_uid`, phase, duration).  
- Failures must be rollback-safe: cleanup mounts, networks, temp dirs.  
- For OCI integration: reuse existing runtime components (e.g., youki, crun) when feasible, clearly define supported features.

## Tests & benchmarks
- Integration tests: minimal container and pod lifecycle.  
- Stress tests: concurrent creation of N pods.  
- Benchmarks: startup latency, resident memory, scalability; use `criterion`.

## Copilot guidance
- Generate Rust runtime code first, then optional CLI sample (`rkl container run …`).  
- Focus templates on process management, cgroup v2, mount orchestration, signal handling, cleanup hooks.
