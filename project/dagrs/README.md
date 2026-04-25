# Dagrs

Welcome to **Dagrs**! Dagrs is an easy-to-use, high-performance asynchronous task programming framework written in Rust.
Dagrs follows the concept of Flow based Programming, and aims to provide users with convenient programming interface.

[Website](https://dagrs.com/) | [Guides](https://dagrs.com/docs/getting-started/introduction) | [Chat](https://discord.gg/4JzaNkRP)

## Project Overview

### Dagrs's role in your project

When you orchestrate multitasking applications asynchronously, you can make them scale better by reducing the cost of performing multiple operations simultaneously.
However, writing correct asynchronous code and managing communication between different tasks is annoying.
Dagrs provides convenient task abstraction and asynchronous running & communication mechanisms to reduce development costs.

The development of `dagrs` follows the concept of Flow-based Programming.

### Flow based Programming
[Flow-based Programming](https://en.wikipedia.org/wiki/Flow-based_programming)(FBP) was invented by J. Paul Morrison in the early 1970s. It was initially implemented in software for a Canadian bank.
Over the years, it's had various names but has always maintained its core principles of reducing development time and managing processes efficiently.

FBP treats applications as networks of 'black box' processes that communicate by sending and receiving data, referred to as Information Packets, over predefined connections. It's a component-oriented approach that fits well with modular software architecture.

### Key Features of FBP

| **Feature**                   | **Description**                                          |
|-------------------------------|----------------------------------------------------------|
| "Black box" Processes         | Encapsulated processes and information packets. |
| Independent Network Construction |The external definition of connections.|
| Asynchronism                   | Asynchronous execution of processes and asynchronous communication.                |
| Ownership and Lifetime        | Information packets with unique ownership and lifetime. |
| Bounded and Finite Connection| Connections between processes are bounded, with a finite capacity.|
| Reverse Pressure | Congestion control when there are too many packets. |


## Technology & Libraries

Dagrs leverages cutting-edge technologies to ensure functionality and performance:

- **[Rust](https://www.rust-lang.org/)** - A language empowering everyone to build reliable and efficient software.
- **[tokio](https://crates.io/crates/tokio)** - An event-driven, non-blocking I/O platform for writing asynchronous I/O backed applications.
- **[async_trait](https://crates.io/crates/async-trait)** - Type erasure for async trait methods.


## Advanced Features

### Conditional Node
Conditional nodes allow you to control task flow execution based on specific conditions. By implementing the `Condition` trait, you can define custom condition logic to determine whether to continue executing subsequent tasks. For example, you can create a validation node that only continues execution when input data meets specific conditions.

```rust
struct MyCondition;
#[async_trait]
impl Condition for MyCondition {
    async fn run(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> bool {
        // Implement condition logic here
        true
    }
}
```

### Loop DAG
Loop DAG allows you to create task graphs with cyclic dependencies. This is particularly useful when you need to repeatedly execute certain tasks until specific conditions are met. For example, you can create an interactive system where certain tasks need to be executed repeatedly based on user input.

```rust
let mut lop = LoopSubgraph::new("loop".to_string(), &mut node_table);
lop.add_node(processor).unwrap();
lop.add_node(consumer).unwrap();
```

### Customized Configuration Parser
Dagrs allows you to design your own parser to define task graphs in custom configuration formats. By implementing the `Parser` trait, you can create parsers for various configuration formats (such as JSON, TOML, or your own custom format) to define task names, dependencies, and execution commands.

For example, in the [dagrs-sklearn](examples/dagrs-sklearn) example project, we provide a custom YAML parser implementation that demonstrates how to create a specialized parser for machine learning workflows. This parser extends the basic YAML format to support additional features specific to machine learning tasks.

```rust
pub struct YamlParser;

impl Parser for YamlParser {
    fn parse_tasks(
        &self,
        file: &str,
        specific_actions: HashMap<String, Box<dyn Action>>,
    ) -> Result<(Graph, EnvVar), ParseError> {
        // Custom parsing logic for machine learning workflow
        Ok((graph, env_var))
    }
}
```

## Examples

### dagrs-sklearn
The example [dagrs-sklearn](examples/dagrs-sklearn) shows how dagrs can help implement machine learning algorithms to train classifiers in a parallel manner. It provides:
1. A custom YAML parser for defining machine learning tasks
2. Integration with scikit-learn for data processing and model training
3. Example workflows for common machine learning tasks

This example shows a typical machine learning workflow with three stages:
- Data preprocessing
- Model training
- Model evaluation

Each stage is defined as a task with its dependencies and execution command. The custom YAML parser handles the configuration and builds the corresponding task graph.

For more detailed info about this example, please see the [notebook.ipynb](examples/dagrs-sklearn/examples/notebook.ipynb) jupyter notebook file.

## Quick Start

Dagrs execution is async-only. Runtime lifecycle is managed by callers.

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = build_graph_somehow();
    let report = graph.async_start().await?;
    assert_eq!(report.status, dagrs::CompletionStatus::Succeeded);
    Ok(())
}
```

- Graph construction APIs (`Graph::add_node`, `Graph::add_edge`, `LoopSubgraph::add_node`) return `Result`.
- Execution entry: `Graph::async_start().await`, which returns an `ExecutionReport`.
- Failures use the structured `DagrsError` + `ErrorCode` model.
- Event subscribers should treat `GraphEvent::ExecutionTerminated` as the final signal.
- `GraphEvent::Progress` is emitted once per completed execution block, not once per completed node.
- `Graph::start_with_runtime()` has been removed.
- Use async channel APIs (`recv_from().await`, `send_to(...).await`, `broadcast(...).await`, `close(...).await`).

## Changelog

### v0.8.1

#### 🚀 Runtime/API Changes

- **Structured Error Model**: `DagrsError` and `ErrorCode` now unify graph build, runtime, checkpoint, and channel failures.
- **Build APIs Return `Result`**: `Graph::add_node`, `Graph::add_edge`, and `LoopSubgraph::add_node` now return `Result`.
- **Execution Reports**: `Graph::async_start()` and checkpoint resume APIs now return `ExecutionReport`.
- **Unified Termination Event**: `GraphEvent::ExecutionTerminated` replaces `GraphFinished`, and `Progress` is now emitted during execution at block granularity.
- **Checkpoint State Model**: `NodeExecStatus` makes checkpointed node state explicit, `Output::empty()` is persisted as success, serialized outputs are replayed to pending downstream nodes on resume, and restored skipped parents are hidden from downstream input selection until they execute again.
- **Hook Contract Cleanup**: `ExecutionHook::on_error` has been removed; failures are reported through `DagrsError` and events.
- **Reset Semantics**: `reset()` now preserves the caller environment by default. Use `reset_with(...)` for explicit environment reset.
- **Loop Checkpoint Restore**: built-in `CountLoopCondition` restores its iteration state from checkpoints; custom loop conditions should override `restore_from_checkpoint(...)` if they carry their own resume-sensitive counters.

#### 💡 Migration

- Replace `GraphError`, `RecvErr`, `SendErr`, `CheckpointError`, and string-based `Output::Err(...)` handling with `DagrsError`.
- Handle or unwrap `Graph::add_node(...)`, `Graph::add_edge(...)`, and `LoopSubgraph::add_node(...)`.
- Update `async_start()` / `resume_from_checkpoint()` call sites to consume the returned `ExecutionReport`.
- Replace `GraphFinished` listeners with `ExecutionTerminated`.
- Move command exit-code-specific error handling into `DagrsError.context` instead of `Output::ErrWithExitCode`.

Example migration for command-style actions:

```rust
// Before (removed)
// Output::ErrWithExitCode("command failed".to_string(), code)

// After
let mut err = DagrsError::new(
    ErrorCode::DgRun0006NodeExecutionFailed,
    format!("command `{}` exited with a non-zero status", command),
);
if let Some(code) = code {
    err = err.with_detail("exit_code", code.to_string());
}
Output::error(err)
```

### v0.8.0

#### 🚀 Runtime/API Changes

- **Async-only Execution**: `Graph::async_start()` is the only execution entry point.
- **Removed Sync Adapter**: `Graph::start_with_runtime(&runtime)` has been removed.
- **Removed Blocking Channel APIs**:
  - `InChannels` / `TypedInChannels`: removed `blocking_recv_from`, `blocking_map`.
  - `OutChannels` / `TypedOutChannels`: removed `blocking_send_to`, `blocking_broadcast`.
- **Internal Async Cleanup**: runtime internals no longer use blocking channel paths.

#### 💡 Migration

- Replace `start_with_runtime(&runtime)` with `async_start().await`.
- Replace blocking channel calls with async methods:
  - `recv_from().await`, `recv_any().await`, `map(...).await`
  - `send_to(...).await`, `broadcast(...).await`, `close(...).await`

### v0.7.0

#### 🚀 Runtime/API Changes

- **Runtime Decoupling**: Dagrs no longer creates or owns a Tokio runtime internally.
- **Primary Entry**: `Graph::async_start()` is the recommended execution entry.
- **Removed API**: `Graph::start()` has been removed.

#### 💡 Migration

- Existing `Graph::start()` callers should migrate to `async_start().await`.

### v0.6.0

#### 🚀 New Features

- **Loop Node (REQ-001)**: Introduced `LoopNode` and `FlowControl::Loop` to support iterative DAG execution.
- **Checkpoint Mechanism (REQ-002)**: Added checkpoint persistence and resume support for graph execution state.
- **Dynamic Router (REQ-003)**: Added `RouterNode` for runtime branch selection and automatic branch pruning.
- **Typed Channels (REQ-004)**: Added typed channel wrappers for safer node-to-node data transfer.
- **Execution Hooks (REQ-006)**: Enhanced `ExecutionHook` with retry lifecycle callbacks.
- **State Subscription (REQ-007)**: Added graph event subscription based on `tokio::sync::broadcast`.

#### 💡 Usage Recommendations

- Use loop nodes and router nodes to model iterative and branching workflows.
- Use checkpoints and event subscriptions when you need resumability and runtime observability.

## Contribution

The `dagrs` project relies on community contributions and aims to simplify getting started. To develop `dagrs`, clone the repository, then install all dependencies, run the test suite and try it out locally. Pick an issue, make changes, and submit a pull request for community review.

### Version Release Notes

When releasing a new version, update `dagrs` and `dagrs-derive` as a compatible pair. They do not need identical version numbers, but breaking changes in one crate that require changes in the other should be released together. For this release line, the compatibility target is `dagrs 0.8.1` with `dagrs-derive 0.5.0`.

For example, as mentioned in issue [#98](https://github.com/dagrs-dev/dagrs/issues/98), the problem occurred because `dagrs` version 0.4.3 depended on a `dagrs-derive` release that did not include the required macro changes. Release the compatibility matrix together so downstream users do not get a mismatched runtime/macro pair.

### What's the contribution

Here are some guidelines for contributing to this project:

1. Report issues/bugs: If you find any issues or bugs in the project, please report them by creating an issue on the issue tracker. Describe the issue in detail and also mention the steps to reproduce it. The more details you provide, the easier it will be for me to investigate and fix the issue.
2. Suggest enhancements: If you have an idea to enhance or improve this project, you can suggest it by creating an issue on the issue tracker. Explain your enhancement in detail along with its use cases and benefits. I appreciate well-thought-out enhancement suggestions.
3. Contribute code: If you want to develop and contribute code, follow these steps:
   - Choose an issue to work on. Issues labeled `good first issue` are suitable for newcomers. You can also look for issues marked `help wanted`.
   - Fork the `dagrs` repository and create a branch for your changes.
   - Make your changes and commit them with a clear commit message. Sign the [Developer Certificate of Origin](https://developercertificate.org/) (DCO) by adding a `Signed-off-by` line to your commit messages. This certifies that you wrote or have the right to submit the code you are contributing to the project.
   - Push your changes to GitHub and open a pull request.
   - Respond to any feedback on your pull request. The `dagrs` maintainers will review your changes and may request modifications before merging. Please ensure your code is properly formatted and follows the same style as the existing codebase.
   - Once your pull request is merged, you will be listed as a contributor in the project repository and documentation.
4. Write tutorials/blog posts: You can contribute by writing tutorials or blog posts to help users get started with this project. Submit your posts on the issue tracker for review and inclusion. High quality posts that provide value to users are highly appreciated.
5. Improve documentation: If you find any gaps in the documentation or think any part can be improved, you can make changes to files in the documentation folder and submit a PR. Ensure the documentation is up-to-date with the latest changes.

Your contributions are highly appreciated. Feel free to ask any questions if you have any doubts or facing issues while contributing. The more you contribute, the more you will learn and improve your skills.

### DCO & PGP

To comply with the requirements, contributors must include both a `Signed-off-by` line and a PGP signature in their commit messages. You can find more information about how to generate a PGP key [here](https://docs.github.com/en/github/authenticating-to-github/managing-commit-signature-verification/generating-a-new-gpg-key).

Git even has a `-s` command line option to append this automatically to your commit message, and `-S` to sign your commit with your PGP key. For example:

```bash
$ git commit -S -s -m 'This is my commit message'
```

### Rebase the branch

If you have a local git environment and meet the criteria below, one option is to rebase the branch and add your Signed-off-by lines in the new commits. Please note that if others have already begun work based upon the commits in this branch, this solution will rewrite history and may cause serious issues for collaborators (described in the git documentation under "The Perils of Rebasing").

You should only do this if:

- You are the only author of the commits in this branch
- You are absolutely certain nobody else is doing any work based upon this branch
- There are no empty commits in the branch (for example, a DCO Remediation Commit which was added using `-allow-empty`)

To add your Signed-off-by line to every commit in this branch:

- Ensure you have a local copy of your branch by checking out the pull request locally via command line.
- In your local branch, run: `git rebase HEAD~1 --signoff`
- Force push your changes to overwrite the branch: `git push --force-with-lease origin main`

## License

Freighter is licensed under this Licensed:

* MIT LICENSE ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)

## Contact us

Email: Quanyi Ma <eli@patch.sh>, Xiaolong Fu <njufxl@gmail.com>

### Discord

Welcome to join our discord channel https://discord.gg/4JzaNkRP

