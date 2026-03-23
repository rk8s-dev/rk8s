## PR Review: Feat/enhance error handle

### 概要

本 PR 对 `dagrs` (0.8.0 → 0.8.1) 和 `dagrs-derive` (0.4.3 → 0.5.0) 进行了大规模的错误处理和 API 契约重构，涵盖 80 个文件、+4174/-2253 行变更。核心改动包括：

1. **统一错误模型**：引入 `DagrsError` + `ErrorCode` 取代分散的 `GraphError`、`RecvErr`、`SendErr`、`CheckpointError`
2. **构建 API 返回 `Result`**：`add_node`、`add_edge`、`LoopSubgraph::add_node` 不再 panic
3. **执行报告**：`async_start()` 返回 `ExecutionReport` 代替 `()`
4. **事件/Hook 契约重构**：`ExecutionTerminated` 取代 `GraphFinished`；移除 `on_error` hook
5. **Checkpoint 增强**：`NodeExecStatus` 状态机、输出序列化回放、纳秒时间戳、序列号去重
6. **dagrs-derive 安全性提升**：reserved field 检测、泛型分片正确性、命名空间前缀避免冲突

---

### 优点

- **结构化错误码设计优秀**：`ErrorCode` 采用 `DgBld`/`DgRun`/`DgChk`/`DgChn` 前缀 + 序号的方案，对运维调试友好，保持了良好的可扩展性。
- **Builder pattern 的 `DagrsError`**：`.with_node()`、`.with_checkpoint()`、`.with_detail()` 链式调用风格简洁且信息丰富。
- **消除 panic 路径**：`add_node`/`add_edge`/`try_lock_node_for_build` 从 panic 改为返回 `Result`，极大提升了库的健壮性。
- **Checkpoint 排序改进**：纳秒时间戳 + 全局 `AtomicU64` 序列号 + `checkpoint_cmp` 三级排序，解决了快速连续创建 checkpoint 时的歧义问题。
- **`dependencies!` 宏返回 `Result`**：变量名加 `__dagrs_` 前缀避免命名冲突，`add_node`/`add_edge` 传播错误而非静默忽略。
- **`unsafe impl Send/Sync` 的移除**：`auto_node` 不再盲目标记 Send+Sync，改为依赖编译器自动推导，更安全。
- **测试覆盖充分**：新增大量针对 checkpoint 恢复、重复 ID 检测、事件流的测试用例。
- **文档和迁移指南完善**：CHANGELOG 和 README 都清晰描述了 breaking changes 和迁移步骤。

---

### 需要关注的问题

#### 1. `run_internal` 参数过多 (高优先级)
`run_internal` 方法有 **8 个参数** (已有 `#[allow(clippy::too_many_arguments)]`)，建议将 `run_id`、`started_at_unix_secs`、`start_pc`、`start_loop_count`、`initial_completed_total`、`initial_skipped_total` 封装到一个 `RunContext` 结构体中。这不仅解决 clippy 警告，还能让未来扩展更容易。

#### 2. `CHECKPOINT_SEQUENCE` 全局 AtomicU64 在多 Graph 场景下的语义 (中优先级)
`checkpoint.rs` 中的 `static CHECKPOINT_SEQUENCE: AtomicU64` 是进程级全局的。如果同一进程中存在多个 `Graph` 实例并发运行，序列号会交叉递增。虽然不会导致正确性问题（因为 checkpoint ID 还包含时间戳和 pc），但从可读性和调试角度考虑，文档中应注明此行为，或考虑将序列号改为 Graph 实例级别。

#### 3. `Output::ErrWithExitCode` 移除的迁移路径 (中优先级)
`Output::ErrWithExitCode` 被移除，README 建议使用 `DagrsError.context` 替代。但 `DagrsError` 并没有提供等价的 `with_exit_code()` 便利方法。对于 `dagrs-sklearn` 等运行外部命令的场景，用户需要通过 `with_detail("exit_code", code.to_string())` 手动传递退出码——建议至少在文档中给出具体的迁移代码示例。

#### 4. `InChannels` 的 tuple struct 设计 (低优先级)
`InChannels` 从 `InChannels(HashMap)` 变为 `InChannels(HashMap, HashSet)`，使用匿名字段 `.0` 和 `.1`。虽然 `InChannels` 主要在 crate 内部使用，但后续维护中 `.0` 和 `.1` 的语义不够直观，建议重构为命名字段：
```rust
pub struct InChannels {
    pub(crate) channels: HashMap<NodeId, Arc<Mutex<InChannel>>>,
    pub(crate) disabled: HashSet<NodeId>,
}
```

#### 5. `aggregate_errors` 丢失多错误详情 (低优先级)
当多个节点失败时，`aggregate_errors` 只保留第一个错误（单错误时）或创建一个只含 `error_count` detail 的聚合错误。后续错误的 `node_id`、`node_name`、`message` 信息丢失了。建议至少在 `details` 中附上每个子错误的摘要，或引入 `DagrsError::sources: Vec<DagrsError>` 字段。

#### 6. `check_loop_and_partition` 返回值语义变更 (提示)
原来 `check_loop_and_partition` 返回 `bool` (是否有环)，现在返回 `DagrsResult<()>` (有环则 Err)。这是正确的改进，当前代码已正确处理所有调用方。

#### 7. `ExecState` 中 poisoned mutex 的处理 (提示)
`execstate.rs` 中将 `unwrap()` 改为 `ok()`/`unwrap_or_else` 来处理 poisoned mutex，这更安全。但如果 mutex 真的 poisoned（说明之前有 panic），静默返回 `Output::empty()` 或 `None` 可能掩盖上游 bug。考虑至少记录一条 `warn!` 日志。

---

### 其他小建议

- `event.rs` 中 `NodeSuccess` 新增了 `duration_ms` 字段，但 `NodeFailed` 没有。失败节点的执行时长对性能诊断同样有用，建议保持对称。
- `dagrs` 版本升级为 `0.8.1`，但本次包含多个 breaking change（`async_start` 返回类型变更、`add_node`/`add_edge` 返回类型变更、移除 `Output::ErrWithExitCode` 等）。根据 SemVer，这应该是 `0.9.0`。不过考虑到 `0.x` 系列的惯例（minor version 允许 breaking），可以接受，但建议在 CHANGELOG 中明确标注为 **breaking release**。
- BUCK 文件统一了 `name` 命名规则（去掉了 `dagrs-` 前缀），这很好，但确认 CI/CD 和其他依赖方已同步更新。

---

### 总结

这是一个 **高质量的重构 PR**，将 dagrs 的错误处理从散乱的 panic + 字符串错误提升为结构化、可编程的错误模型。改动范围大但逻辑清晰，文档和测试都跟进到位。上述建议主要是进一步的增强，不阻碍合并。

**建议：Approve with minor suggestions** ✅
