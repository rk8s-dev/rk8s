# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **AI Agent Support (REQ-008)**: Introduced a lightweight, native AI module (feature `ai`) for integrating LLMs into DAG workflows.
  - Implemented generic `Agent` and `CompletionModel` traits for LLM abstraction.
  - Added `AgentAction` adapter to wrap AI agents as standard executor nodes.
  - Provided built-in support for Google Gemini via `GeminiProvider`.
  - Defined unified message structures (`Message`, `CompletionRequest`) with future-proof design for Tools and Multimodal inputs.

## [0.6.0] - 2026-02-03

### Added
- **Loop Node (REQ-001)**: Introduced `LoopNode` struct and `LoopCondition` trait to support controllable iterative loops. Added `FlowControl::Loop` instruction for execution flow management.
- **Checkpoint Mechanism (REQ-002)**: Implemented `Checkpoint` struct and `FileCheckpointStore` for state persistence. Added support for capturing and restoring graph execution snapshots (active nodes, env, loop counters).
- **Dynamic Router (REQ-003)**: Added `RouterNode` implementing the `Router` trait for runtime conditional branching. Implemented automatic branch pruning for unselected paths.
- **Typed Channels (REQ-004)**: Added `TypedInChannels` and `TypedOutChannels` wrappers to enforce compile-time type safety for data transfer between nodes.
- **Execution Hooks (REQ-006)**: Enhanced `ExecutionHook` trait with `on_retry` method. Updated `Graph::run` to invoke hooks at key lifecycle events (start, success, fail, retry).
- **State Subscription (REQ-007)**: Implemented an event bus using `tokio::sync::broadcast`. Added `GraphEvent` enum (NodeStart, NodeSuccess, etc.) and a public `subscribe()` method for real-time monitoring.

### Planned
- **Visualization (REQ-005)**: Export DAG structure to DOT/Mermaid format (Scheduled for next release).

## [0.5.2] - 2024-01-29

### Added
- Initial project structure and core DAG engine implementation.
