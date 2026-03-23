# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.1] - 2026-03-23

### Changed
- **Unified Error Contract**:
  - Introduced `DagrsError` and `ErrorCode` as the stable public error surface.
  - Replaced string-based node errors and channel/checkpoint-specific public error enums with structured errors.
  - Command-style example errors now carry exit metadata through `DagrsError.context`.
- **Build API Contract**:
  - `Graph::add_node(...)`, `Graph::add_edge(...)`, and `LoopSubgraph::add_node(...)` now return `Result`.
  - Graph construction paths now reject duplicate node IDs and invalid edges with stable error codes instead of panicking.
- **Execution Contract**:
  - `Graph::async_start()` and checkpoint resume APIs now return `ExecutionReport`.
  - `reset()` preserves caller-provided environment state by default; `reset_with(...)` provides explicit reset control.
  - Runtime panic and join failures are surfaced through structured runtime error codes.
- **Event / Hook / Checkpoint Contract**:
  - `GraphEvent::ExecutionTerminated` replaces `GraphFinished` as the final execution signal.
  - `Progress` events are emitted during execution at block granularity.
  - `ExecutionHook::on_error` has been removed; failures are reported via `DagrsError`, `NodeFailed`, and `ExecutionTerminated`.
  - Checkpoint node state now uses `NodeExecStatus`, `Output::empty()` is persisted as successful completion, serializable outputs are replayed into downstream channels on resume, and restored skipped parents are hidden from downstream input selection until they execute again.
- **Examples & Tests**:
  - Updated examples, `dagrs-sklearn`, tests, and docs to the new build/runtime/error APIs.

### Removed
- `Output::ErrWithExitCode(...)`.
- `GraphEvent::GraphFinished`.
- `CheckpointConfig::before_conditional`.
- `ExecutionHook::on_error`.
- Public reliance on `GraphError`, `RecvErr`, `SendErr`, and `CheckpointError`.

### Migration
- Replace:
  - `graph.add_node(node);`
  - `graph.add_edge(from, to);`
  - `loop_subgraph.add_node(node);`
  - `let _: () = graph.async_start().await?;`
  - `GraphEvent::GraphFinished`
- With:
  - `graph.add_node(node)?;`
  - `graph.add_edge(from, to)?;`
  - `loop_subgraph.add_node(node)?;`
  - `let report = graph.async_start().await?;`
  - `GraphEvent::ExecutionTerminated { .. }`

## [0.8.0] - 2026-03-17

### Changed
- **Async-only Execution API**: `Graph::async_start()` is now the only execution entry point.
- **Async-only Channel API**:
  - Removed all blocking channel operations from `InChannels` / `OutChannels`.
  - Removed all blocking channel operations from `TypedInChannels` / `TypedOutChannels`.
  - Channel closing is now asynchronous (`close(...).await`, `close_all().await` in internal paths).
- **Examples & Tests**:
  - Migrated all runtime-based sync invocation examples to async (`#[tokio::main]` + `async_start().await`).
  - Migrated graph execution tests to async style (`#[tokio::test]`) and removed runtime `block_on` wrappers.

### Removed
- `Graph::start_with_runtime(&runtime)` sync adapter.
- `GraphError::BlockingCallInAsyncContext` (no longer needed after sync adapter removal).
- Internal blocking channel paths (`blocking_lock` / `blocking_recv` / `blocking_send`) in `dagrs` runtime code.

### Migration
- Replace:
  - `graph.start_with_runtime(&runtime)?`
- With:
  - `graph.async_start().await?`
- Replace any blocking channel usage with async equivalents:
  - `recv_from().await`, `recv_any().await`, `map(...).await`
  - `send_to(...).await`, `broadcast(...).await`, `close(...).await`

### Planned
- **Visualization (REQ-005)**: Export DAG structure to DOT/Mermaid format (Scheduled for next release).

## [0.7.0] - 2026-03-16

### Added
- **Explicit Sync Adapter**: `Graph::start_with_runtime(&runtime)` for synchronous callers with an externally managed Tokio runtime.
- **Runtime Nesting Guard**: `ensure_blocking_context()` prevents accidental `start_with_runtime` calls from within async contexts, returning `GraphError::BlockingCallInAsyncContext`.
- **Async Example**: Added `hello_dagrs_async.rs` demonstrating the recommended `#[tokio::main]` + `async_start().await` pattern.

### Changed
- **Runtime Decoupling**: The library no longer creates or owns a Tokio runtime. `Graph::async_start()` is now the primary execution entry point. Runtime lifecycle is entirely managed by the caller.
- All examples migrated to `#[tokio::main]` + `async_start().await` or explicit `Runtime::new()` + `start_with_runtime` patterns.
- Graph construction paths (`add_node` / `add_edge`) now avoid `blocking_lock`, fixing async-context panics during graph building.
- `GraphError::RuntimeCreationFailed` removed with `Graph::start()` removal.

### Deprecated
- `Graph::start_with_runtime()` is now deprecated as a legacy sync adapter and is planned for removal in the next major version. Prefer `async_start().await`.

### Removed
- `Graph::start()` removed. Use `async_start().await` or `start_with_runtime(&runtime)`.

### Planned
- **Visualization (REQ-005)**: Export DAG structure to DOT/Mermaid format (Scheduled for next release).

## [0.6.0] - 2026-02-01

### Added
- **Loop Node (REQ-001)**: Introduced `LoopNode` struct and `LoopCondition` trait to support controllable iterative loops. Added `FlowControl::Loop` instruction for execution flow management.
- **Checkpoint Mechanism (REQ-002)**: Implemented `Checkpoint` struct and `FileCheckpointStore` for state persistence. Added support for capturing and restoring graph execution snapshots (active nodes, env, loop counters).
- **Dynamic Router (REQ-003)**: Added `RouterNode` implementing the `Router` trait for runtime conditional branching. Implemented automatic branch pruning for unselected paths.
- **Typed Channels (REQ-004)**: Added `TypedInChannels` and `TypedOutChannels` wrappers to enforce compile-time type safety for data transfer between nodes.
- **Execution Hooks (REQ-006)**: Enhanced `ExecutionHook` trait with `on_retry` method. Updated `Graph::run` to invoke hooks at key lifecycle events (start, success, fail, retry).
- **State Subscription (REQ-007)**: Implemented an event bus using `tokio::sync::broadcast`. Added `GraphEvent` enum (NodeStart, NodeSuccess, etc.) and a public `subscribe()` method for real-time monitoring.

## [0.5.2] - 2024-01-29

### Added
- Initial project structure and core DAG engine implementation.
