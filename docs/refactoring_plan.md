# RKL & Rkforge Refactoring Plan

This document outlines the plan to refactor `rkl` and `rkforge` to separate their roles and consolidate shared logic.

## Goal
- **rkforge**: Transform into the primary local developer CLI (Build, Push, Pull, Run, Compose).
- **rkl**: Focus strictly on being the worker node daemon (CRI implementation, Cluster worker) for `rks`.
- **common**: Extract core runtime and lifecycle logic into a shared library.

---

## 1. Shared Library Strategy (`common` crate)
Extract reusable logic from `rkl` into `project/common`.

- **Module**: `project/common/src/runtime/`
- **Content**:
  - Generic container lifecycle: `create`, `start`, `stop`, `kill`, `delete`, `list`, `state`, `exec`.
  - Pod sandbox management (pause container logic).
  - OCI specification generation helpers.
- **Module**: `project/common/src/compose/`
  - Move the entire `compose` logic from `rkl`.

---

## 2. Work Division

### Developer 1: Core Extraction & Shared Lib
- Move helper functions from `rkl/src/commands/mod.rs` to `common/src/runtime/`.
- Decouple logic from `clap` CLI structures to make it library-friendly.
- Move `rkl/src/commands/compose/` to `common/src/compose/`.
- Ensure all required dependencies are moved to `common/Cargo.toml`.

### Developer 2: Rkforge Enhancement (The Developer CLI)
- Add `container`, `pod` (local/standalone), and `compose` subcommands to `rkforge`.
- Integrate the shared runtime/compose logic from `common`.
- Implement `rkforge run` (similar to `docker run`) and `rkforge compose up/down`.
- Update dependencies in `rkforge/Cargo.toml`.

### Developer 3: RKL Purifier (The Worker Node)
- Remove `container` and `compose` subcommands from `rkl`.
- Remove standalone/local pod management from `rkl`.
- Refactor `rkl/src/task.rs` to use the new shared library.
- Focus `rkl` on the `daemon` mode and CRI-compatible operations received from `rks`.
- Clean up unused dependencies in `rkl/Cargo.toml`.

---

## 3. Component Comparison

| Component | Before Refactor | After Refactor |
| :--- | :--- | :--- |
| **rkl** | Local CLI + Worker Node | **Worker Node (Daemon) Only** |
| **rkforge** | Builder + Registry CLI | **Builder + Registry + Local Runtime CLI** |
| **common** | Utilities (Quic, Lease) | Utilities + **Core Runtime Logic + Compose** |
