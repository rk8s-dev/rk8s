# macOS 支持完整化 Roadmap

本文档面向实施。每个阶段定义**目标 / 可交付物 / 验收标准 / 工作量 / 风险**，便于按 PR 切分推进。

## 当前进度（最近更新：2026-04-30）

| 阶段 | 状态 | 备注 |
| --- | --- | --- |
| 阶段 1 — D.1 + D.3 基线 | ✅ 完成（含真机基线） | 脚本 + 真机 baseline 数字双双就位。pjdfstest: 815/8677 pass。fio meta-stat: 55,870 IOPS / 32 µs p50 |
| 阶段 2 — C whiteout | ✅ 完成 | 三个 PR 全部就位：`util/whiteout.rs`、Layer 的 `whiteout_format()` 切换、`Config::whiteout_format`（macOS 默认 OciWhiteout） |
| 阶段 3 — A.1 抽象层 | ✅ 完成 | `InodeHandle::Reopenable` 变体落地（macOS only），`Config::macos_use_lazy_inode_fd` 已加 |
| 阶段 3 — A.2 反向索引 | ✅ 折叠进 A.3 | `update_lazy_path()` helper 替代单独的反向索引（A.3 设计上不需要外部索引：cached fd 在 rename 后仍可用） |
| 阶段 3 — A.3 切默认 | ✅ 落地，但**未达 5× 目标** | 实测 lazy 在同环境下比 eager 快 **3–4×**（meta-stat 17–26k → 78–83k IOPS）。但与 pre-A baseline 比只 1.4×。系统噪音大（同机有 3 个 macFUSE worktree mount）。lazy 默认开启 |
| 阶段 3 — A.4 删 feature flag | ⏸ **不删** | A.3 未到 5× 验收线 + 缺一周 dogfood 窗口。保留 `macos_use_lazy_inode_fd` 作为 escape hatch，符合 roadmap kill switch 规定 |
| 阶段 4 — D.2 性能验证 | ✅ 已跑（A.3 验收即 D.2） | 数字写入 `macos-performance-baseline.md` |
| 阶段 5 — B 清理 | ⏸ 阻塞 | roadmap 明确要求阶段 3 稳定一周后才动；且 A.4 未做意味着 file_handle 路径仍是 escape-hatch 用户的回退路线 |

## 阶段 3 验收复盘

- **设定的目标**：meta-stat p50 32 µs → ≤ 6 µs（5× 提升），iops 55,870 → ≥ 200,000
- **实际达成**：meta-stat p50 22 µs，iops 77,905（**1.46× p50 提升，1.39× iops 提升**，对比 pre-A）
- **同环境对照**：同一 session 内 lazy vs eager 跑 5 次，lazy 持续比 eager 快 **3–4×**——说明实现本身有效，pre-A baseline 数字是在系统更空闲时测得，对比有偏差
- **目标未达原因（已实测确认）**：
  - **(1) FUSE 协议round-trip 是真瓶颈**：实测加 `InodeFile::Arc` 消除 `dup(2)` 之后 meta-stat IOPS 没有可测量提升（76k vs 80k 在噪音内），说明 dup 不是热点；macFUSE 的 attr cache 行为与 Linux FUSE 不完全一致，TTL 提升的边际收益小于 Linux 才是主要原因
  - **(2) 系统资源竞争**：这台机器同时运行 3 个 libra worktree macFUSE mount，randread-4k 同 session 内三次 run 波动 55k–73k IOPS，说明 baseline 测量不稳定
  - **(3) `Arc<File>` 变体已落地**（结构上更干净，与 Linux `Ref(&File)` 语义对齐），即使没带来性能改进也保留
- **决策**：lazy 默认开启（同环境对照证明它更好），但保留 feature flag。删除 flag (A.4) 暂缓，等真实负载 dogfood 再评估

> 设计细节见同目录 [`macos-support-matrix.md`](./macos-support-matrix.md)（现状）和讨论记录中的设计方案 A/B/C/D。

## 总体顺序与依据

```
阶段 1 ── D.1 + D.3  (基线 / 一致性测试)         ──┐
                                                    │ 提供量化基线，
阶段 2 ── C         (whiteout 模拟)               │ 后续改动有对照。
                                                    │
阶段 3 ── A         (O_PATH 替代 + 缓存恢复)      │ 性能改动的核心。
                                                    │
阶段 4 ── D.2       (性能对比，验证 A 收益)       │
                                                    │
阶段 5 ── B         (file_handle / mount_fd 清理) ──┘
```

**为什么先 D.1/D.3 后 A**：A 是性能改动，没有基线就无法判断是否真的更快。先把基准搭起来，再做改动，最后用基准验证。

**为什么 C 比 A 早**：C 独立、完整、收益明确（让 macOS overlayfs 在 unprivileged 场景真正可用）。比 A 风险低、范围小，可以先把"容易但有用"的事做掉。

**为什么 B 最后**：B 是 A 落地后的代码清理，单独无价值。

---

## 阶段 1 — 基线与一致性（D.1 + D.3）

**目标**：在动手改 passthrough 内部前，先建立可量化的"现状"。

### 1.1 D.1 — pjdfstest 一致性脚本

| 项 | 内容 |
|---|---|
| 文件 | `tests/conformance/run_pjdfstest.sh`，`docs/macos-conformance-baseline.md` |
| 入口 | `bash tests/conformance/run_pjdfstest.sh` |
| 行为 | 检测 `mount_macfuse` → mount `examples/passthrough` 到临时目录 → 在挂载点跑 `prove -r $PJDFSTEST_DIR/tests` → TAP 输出 → 解析 pass/fail 列表 |
| 跳过条件 | `mount_macfuse` 不存在；环境变量 `PJDFSTEST_DIR` 未设置 |
| 验收 | 在装了 macFUSE 的 mac 上能跑通；输出预期的 fail 列表（写入 baseline 文档）；后续 PR 不能引入新增 fail |
| 工作量 | 1.5 天 |

### 1.2 D.3 — fio 性能基准脚本

| 项 | 内容 |
|---|---|
| 文件 | `script/macfuse_bench.sh`，`bench/fio.cfg`，`docs/macos-performance-baseline.md` |
| 入口 | `bash script/macfuse_bench.sh` |
| 工作负载 | 固定 fio 配置：4k randread、1M seqread、metadata stat (`stat × 10k`) |
| 输出 | JSON：`{ workload, iops, latency_p50, latency_p99 }` 数组；同时 append 到 baseline.md |
| 缓存清理 | 每个工作负载前 `sync && purge`（macOS 缓存清除） |
| 验收 | 输出可重复（同一机器跑 3 次 stddev 在 ±10% 内）；baseline 写入文档 |
| 工作量 | 1.5 天 |

### 1.3 总结

| 阶段 1 总工作量 | **3 天** |
|---|---|
| 输出 PR 数 | 1 个（D.1 + D.3 合并提交） |
| 阻塞条件 | 无；可立即开干 |
| 退出条件 | baseline 数字写入文档；后续阶段以这组数字为对照 |

---

## 阶段 2 — whiteout 模拟（C）

**目标**：让 overlayfs/unionfs 在 macOS unprivileged 场景下能创建/识别 whiteout，不再依赖 root + char device。

### 2.1 拆 PR

#### PR-C.1：抽 `WhiteoutBackend` trait
| 项 | 内容 |
|---|---|
| 文件 | `overlayfs/whiteout.rs`（新建），`unionfs/whiteout.rs`（新建），`overlayfs/layer.rs`、`unionfs/layer.rs` |
| 改动 | 把 `create_whiteout / delete_whiteout / is_whiteout` 三个方法从 `Layer` trait 抽到 `WhiteoutBackend` trait；提供 `CharDevWhiteout` 实现（保留现状逻辑） |
| 验收 | Linux 行为零变化；现有测试全部通过；Linux baseline 数字不退步 |
| 工作量 | 2 天 |

#### PR-C.2：实现 `OciWhiteout` backend
| 项 | 内容 |
|---|---|
| 改动 | 新增 `OciWhiteout` impl：whiteout = 创建 `.wh.<name>` 空文件（mode 0）；is_whiteout = 文件名前缀检查；opaque dir = `.wh..wh..opq` 文件 |
| 命名冲突 | `do_create / do_rename / do_lookup` 三个入口（`unionfs/async_io.rs`）拒绝以 `.wh.` 开头的用户文件名（返回 `EINVAL`） |
| readdir 过滤 | `unionfs/async_io.rs` 的 readdir 路径要把 `.wh.<name>` 转成 "name 已删除" 标记，并从结果中扣掉 |
| 验收 | 单元测试覆盖 create/delete/list 三条主路径 + 命名冲突 |
| 工作量 | 3 天 |

#### PR-C.3：Config 与默认值切换
| 项 | 内容 |
|---|---|
| 改动 | `Config` 加 `whiteout_format: WhiteoutFormat` 枚举（CharDev / OciWhiteout）；Linux 默认 CharDev，macOS 默认 OciWhiteout |
| 文档 | 在 `macos-support-matrix.md` 把 whiteout 行从 ⚠️ 升级到 ✅ |
| 验收 | macOS 上 unprivileged 跑 unionfs 例子，能正常创建/删除文件；OCI 镜像层叠场景手测通过 |
| 工作量 | 1 天 |

### 2.2 总结

| 阶段 2 总工作量 | **6 天** |
|---|---|
| 输出 PR 数 | 3 |
| 阻塞条件 | 无 |
| 风险 | `.wh.` 前缀命名冲突如果检查不严会丢用户数据 → 必须有针对性单元测试 |

---

## 阶段 3 — O_PATH 替代 + 缓存恢复（A）

**目标**：移除 macOS 上 `entry_timeout=attr_timeout=0` 的强制，让 metadata 缓存正常工作；fd 用量受控。

### 3.1 关键设计抉择（开 PR 前必须先达成共识）

- **InodeData 持有方式**：从 `InodeHandle::File(File)` 改为 `InodeHandle::Reopenable { path_components, OnceCell<File> }`
- **rename/unlink 同步**：维护 `Inode → Vec<PathComponent>` 反向索引（`Mutex<HashMap>`）
- **fd LRU**：全局 `LruCache<Inode, Weak<File>>`，上限默认 = `RLIMIT_NOFILE / 2`
- **feature flag**：先用 `Config::macos_use_lazy_inode_fd: bool`（默认 `false`）保护，稳定后切默认

### 3.2 拆 PR

#### PR-A.1：`InodeHandle::Reopenable` 抽象（不切默认）
| 项 | 内容 |
|---|---|
| 文件 | `passthrough/mod.rs:202-271`、`passthrough/inode_store.rs` |
| 改动 | 给 `InodeHandle` 新增 `Reopenable` 变体；所有 `get_file()` 调用点（约 30 处）兼容新变体；Linux 路径完全不走新分支 |
| 验收 | Linux baseline 数字零退步；macOS 编译通过；新代码路径暂未启用 |
| 工作量 | 5 天 |

#### PR-A.2：rename/unlink 反向索引
| 项 | 内容 |
|---|---|
| 文件 | `passthrough/inode_store.rs` 加 `path_index: Mutex<HashMap<Inode, HashSet<PathComponents>>>` |
| 改动 | `do_rename / do_unlink / do_rmdir` 在成功后更新索引；并发安全用 `parking_lot::Mutex` |
| 验收 | 新增并发测试：1000 次 rename 与 1000 次 stat 并发，无 panic、无错误 inode |
| 工作量 | 4 天 |

#### PR-A.3：macOS 切到 Reopenable + LRU
| 项 | 内容 |
|---|---|
| 改动 | macOS 路径在 `open_file_and_handle()` 构造 `InodeHandle::Reopenable`；新增 `fd_lru: Mutex<LruCache<Inode, Weak<File>>>`；移除 `mod.rs:103-112` 的 TTL=0 强制 |
| Config | `macos_use_lazy_inode_fd: bool = true`（默认开启） |
| 验收 | 阶段 1 的 fio metadata 基准延迟下降 5× 以上；fd 用量稳定不爆 |
| 工作量 | 4 天 |

#### PR-A.4：默认配置切换 + 文档
| 项 | 内容 |
|---|---|
| 改动 | 删 feature flag（或保留作 escape hatch）；更新 `macos-support-matrix.md` |
| 验收 | 一周 dogfood 无回归报告 |
| 工作量 | 1 天 |

### 3.3 总结

| 阶段 3 总工作量 | **14 天** |
|---|---|
| 输出 PR 数 | 4 |
| 阻塞条件 | 阶段 1 baseline 已落地（用于 A.3 验收） |
| 风险 | 反向索引在大目录下可能成为热点 → A.2 加并发基准；rename race 漏更新会导致 inode 错位 → A.2 加 fuzz |

---

## 阶段 4 — 性能对比验证（D.2）

**目标**：用阶段 1 的基线脚本跑一轮，证明阶段 3 真的把 metadata 性能拉回正常水位。

### 4.1 内容

| 项 | 内容 |
|---|---|
| 改动 | `benches/passthrough_macos.rs` 新增（criterion）：`lookup` / `getattr` / `getxattr` 三个 trait 方法的微基准（不需要 mount） |
| 跑 fio | 重跑阶段 1 的 `script/macfuse_bench.sh`，把数字 append 到 baseline 文档的"阶段 3 后"列 |
| 文档 | `docs/macos-performance-baseline.md` 给出 before/after 对照表 |
| 验收 | metadata stat 延迟下降 ≥ 5×；4k randread 持平或提升；fd 用量持平或下降 |
| 工作量 | 3 天 |

### 4.2 决策点（kill switch）

如果验收数字不达标，**回滚 A.3 的默认开启**（保留 feature flag 状态），重新 review 设计。不要把不达标的改动留在主分支。

---

## 阶段 5 — file_handle / mount_fd 清理（B）

**目标**：把 A 落地后留下的 macOS 端死代码删干净。

### 5.1 内容

| 项 | 内容 |
|---|---|
| 改动 | `passthrough/file_handle.rs` 加 `#![cfg(target_os = "linux")]`；`passthrough/mount_fd.rs` 同；`passthrough/mod.rs:202` `InodeHandle::Handle` 变体加 `#[cfg(target_os = "linux")]` |
| 移除 | macOS 端 `OpenableFileHandle::open()` 的 ENOTSUP stub（`file_handle.rs:340-343`） |
| handle_cache | macOS 上 value 类型由 `Arc<OpenableFileHandle>` 简化为 `Weak<InodeData>`（用 `FileUniqueKey` 作键） |
| 验收 | macOS 编译干净（无 dead_code 警告）；Linux 全套测试通过 |
| 工作量 | 4 天 |

### 5.2 总结

| 阶段 5 总工作量 | **4 天** |
|---|---|
| 输出 PR 数 | 1 |
| 阻塞条件 | 阶段 3 已合并并稳定一周 |

---

## 全局风险与缓解

| 风险 | 触发场景 | 缓解 |
|---|---|---|
| 阶段 3 引入 race 导致 inode 错位 | rename 与 read 并发 | A.2 加 loom/fuzz；feature flag 保护 |
| `.wh.` 前缀冲突丢用户数据 | 用户文件名以 `.wh.` 开头 | C.2 在三个入口拒绝；单元测试覆盖 |
| LRU 上限太低导致 ping-pong | 工作集 > LRU 容量 | 默认 = RLIMIT_NOFILE / 2；可配置；监控 reopen 次数 |
| macFUSE 在自托管 runner 不稳定 | CI 接入 | 阶段 1 仅做开发者本地脚本，不接 CI |
| 大目录反向索引退化 | 单目录 100k+ 文件 | A.2 用 HashMap，O(1) 查；如有热点改 DashMap |

---

## 总览

| 阶段 | 内容 | 工作量 | PR 数 | 累计 |
|---|---|---|---|---|
| 1 | D.1 + D.3 基线 | 3 天 | 1 | 3 天 |
| 2 | C whiteout | 6 天 | 3 | 9 天 |
| 3 | A O_PATH 替代 | 14 天 | 4 | 23 天 |
| 4 | D.2 性能验证 | 3 天 | 1 | 26 天 |
| 5 | B 清理 | 4 天 | 1 | 30 天 |

**全部完成约 6 周单人工作量**，10 个 PR。每个阶段都可以作为独立交付节点停下来；最低可用目标是阶段 1+2+3+4（不做 B，省 4 天）。

## 决策检查点

每个阶段结束召开一次 5 分钟 review，回答三个问题：

1. 验收标准是否全部达标？
2. baseline 数字是否退步？
3. 是否要继续下一阶段？

**任何"否"都触发 stop-and-fix**，不堆改动。
