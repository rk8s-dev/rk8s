# RK8s – Repository-wide Custom Instructions for GitHub Copilot

## What this repository is
RK8s is a **lightweight, Rust-based Kubernetes-compatible orchestration system**.
It includes:
- **RKL** – a container runtime and pod engine  
- **RKS** – a control plane with scheduler and state management  
- **libbridge/IPAM** – a network plugin providing CNI-style bridge and IP management  

When generating code or designs, assume: container orchestration context, Kubernetes semantics, Rust ecosystem, performance and memory efficiency, and edge/development environments.

## Languages & defaults
- **Rust 2021+** as the primary language.  
- Async runtime: **Tokio**; Logging: **tracing**.  
- Errors: `thiserror` for libraries, `anyhow` for tools/tests.  
- CLI: **clap**.  
- Use `unsafe` only when strictly necessary, and always include a `// SAFETY:` comment with justification and tests.  
- Bash/Python may be used for DevOps or setup scripts, but core logic should remain in Rust.

## Build & run
- Local workflow:  
  `cargo build -p <crate>` · `cargo test -p <crate>` · `cargo bench -p <crate>`  
- Multi-component builds (RKL + RKS + plugins): provide examples or scripts (Buck2/Bazel optional).  
- Example commands: `rkl container run …`, `rkl pod run …`, `rks up …`.

## Code style & quality
- Use `rustfmt` defaults.  
- New code should build warning-free with `cargo build --all-targets`.  
- Treat `clippy` warnings as errors for new code.  
- Avoid `unwrap()` / `expect()` in library code.  
- Use iterators, slices, and streaming I/O instead of large allocations.  
- Benchmark hot paths with `criterion`; include allocation and throughput metrics.

## Observability & errors
- Use `tracing` spans/fields for container, pod, scheduling, and network events.  
- Avoid logging sensitive data.  
- Make errors actionable — include context and remediation hints.

## Testing
- Add unit, integration, and property-based tests (`proptest`).  
- Async/concurrency tests via `#[tokio::test]`.  
- Cover failure scenarios (cgroup limits, CNI failures, image pull errors).  
- Use `insta` snapshots for textual or structured outputs.

## API / CLI / Docs
- CLI defaults to safe operations; add `--dry-run` and `--json` modes.  
- Public APIs should document versioning and stability.  
- Docs in English, concise and practical, including quickstart, architecture diagrams, and troubleshooting.

## Git workflow & PRs
- **Trunk-based development** with **Conventional Commits** (`feat:`, `fix:`, `perf:` …).  
- PRs should include: motivation, design decisions, test coverage, and backward-compatibility notes.  
- Performance-related PRs must include benchmarks and comparisons.

## How Copilot should assist
- When asked for code → produce **Rust first**, with async/await and tracing usage.  
- When asked for design → list multiple options with trade-offs (perf, memory, compatibility, complexity).  
- When asked for tests/benchmarks → include minimal examples using `criterion` or `proptest`.  
- Always assume the RK8s architecture and Kubernetes-like ecosystem.
