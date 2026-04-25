# Compaction & GC — Etcd MetaStore

> Date: 2026-04-17  
> Scope: 在 etcd backend 中补齐 Compaction/GC 相关的 9 个 MetaStore 接口。

## 背景

Compaction 与 GC 的业务逻辑已经在通用层完成，后端只需实现 MetaStore 接口。

当前状态是：database 已实现，etcd 尚未实现这 9 个接口（默认 NotImplemented）。结果是 compaction/GC 在 etcd 场景下基本处于降级状态。

让 etcd 在行为上与 database 对齐，恢复 compaction 与 GC 的有效性和可重试性。

## 需要补齐的接口

1. list_chunk_ids(limit)
2. replace_slices_for_compact(chunk_id, new_slices, old_slices_to_delay)
3. replace_slices_for_compact_with_version(chunk_id, new_slices, old_slices_to_delay, expected_slices)
4. record_uncommitted_slice(slice_id, chunk_id, size, operation)
5. confirm_slice_committed(slice_id)
6. process_delayed_slices(batch_size, max_age_secs)
7. confirm_delayed_deleted(delayed_ids)
8. cleanup_orphan_uncommitted_slices(max_age_secs, batch_size)
9. delete_uncommitted_slices(slice_ids)

## 语义边界

1. 原子性：slice 替换与延迟删除记录写入必须原子。
2. 并发控制：heavy compaction 需要 expected_slices 校验，不一致返回 ContinueRetry。
3. 幂等性：GC 的 delayed/orphan 处理必须支持重复执行，不依赖单次成功。
4. 可恢复性：heavy compaction 写块前先记录 uncommitted，成功后确认，失败后可被 GC 接管。
5. 限流：涉及扫描的接口必须尊重 limit/batch_size。

## 数据模型约束

核心上只需要保证两类记录能力，不强制 key 设计细节：

1. delayed_slice
状态语义：pending -> meta_deleted -> 删除。
用途：两阶段删除中的中间态与重试来源。

2. uncommitted_slice
状态语义：pending -> orphan -> 删除。
用途：heavy compaction 崩溃/回滚后的块清理恢复。

### 当前 etcd slice 存储模型

与 database 的逐行存储不同，etcd 中每个 chunk 的 slices 以 `Vec<SliceDesc>` 的形式
存储在单一 key 下（`key_for_slice(chunk_id)`）。

这意味着：
- `replace_slices_for_compact*` 的实现是"读取 Vec → 修改 → 写回"，而非逐 key 删除/插入。
- `process_delayed_slices` 和 `cleanup_orphan_uncommitted_slices` 中对 slice 存在性的检查，
  也是检查该 Vec 中是否包含对应 slice_id。
- `get_slices` / `append_slice` / `write` / `next_id` 已在 etcd 中实现，无需重复。

`list_chunk_ids` 需要通过 prefix scan（如 `slices/` 前缀）反推 chunk_id 集合。
**顺序要求**：不要求排序，但在相同数据下多次调用结果必须稳定。
**limit 要求**：返回数量不得超过 `limit`，超出部分留到下一周期处理。

delayed_slice 的 id 分配可复用 `generate_id` 机制，或自行决定唯一策略。

## 实现取向
要求：

1. 与当前 etcd 存储模型兼容。
2. 使用现有事务能力保证 CAS/重试语义。
3. 对外行为与 database 版本一致（尤其是冲突返回和 GC 可重试）。

## 验收口径

实现 PR 必须逐条满足以下条件：

| # | 验收项 | 通过条件 |
|---|--------|---------|
| 1 | Light/Heavy Compactor | `tests/compactor_test.rs` 全通过；Heavy 冲突时返回 `ContinueRetry`，但是如果ContinueRetry 没有用就fast fail，不 panic 或死循环。 |
| 2 | GC 延迟清理 | `tests/gc_test.rs` 全通过；`delayed_slice` 支持跨周期重试，无元数据泄漏。 |
| 3 | GC 孤儿清理 | `cleanup_orphan_uncommitted_slices` 能正确识别 pending->orphan，并清理对应块数据。 |
| 4 | Worker 调度 | `tests/compaction_worker_test.rs` 全通过；锁获取/释放与 database 行为一致。 |
| 5 | 端到端一致性 | `tests/gc_compact_e2e_test.rs` 全通过；compaction + GC 后读写数据无损坏。 |
| 6 | 功能不退化 | `CompactionWorker` 和 `BlockStoreGC` 在 etcd 后端下不再跳过或报错 `NotImplemented`。 |

建议测试入口：

1. `tests/compactor_test.rs`
2. `tests/gc_test.rs`
3. `tests/compaction_worker_test.rs`
4. `tests/gc_compact_e2e_test.rs`

## 参考位置

1. 业务调用方：src/chunk/compact/
2. trait 定义：src/meta/store.rs
3. database 对照实现：src/meta/stores/database/mod.rs
4. etcd 待实现位置：src/meta/stores/etcd/mod.rs
5. etcd 事务封装：src/meta/stores/etcd/txn.rs
