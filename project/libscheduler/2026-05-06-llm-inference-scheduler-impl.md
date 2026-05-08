# LLM 推理调度器实施计划

> **状态**：✅ 已实施完成（2026-05-07）。本文为「计划 + 实施差异」合并版本。下方各 Task 的「**实施差异**」框记录了实际落地与原计划的偏离点；不带该框的步骤即按原计划执行。详细总览见文末「实施差异总览」。

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**：按 [`2026-04-11-llm-inference-scheduler-design.md`](./2026-04-11-llm-inference-scheduler-design.md) 设计，给 `libscheduler` 加上 GPU 资源、Gang Scheduling、拓扑感知调度三项能力，并把链路打通到 `with_xline`。

**架构**：在现有 Pod-级 scheduler framework 之上扩展数据模型、新增两个插件、把 `assume → send Assignment` 解耦成「assume → wait gang → send all」。新增的 `GangStateStore` 是跨调度周期的内存共享状态，由 `Scheduler` 持有，注入到主循环和 `TopologyCoAffinityFilter` 插件。

**技术栈**：Rust 1.x、tokio、`Arc<RwLock<...>>`、现有 `libscheduler::plugins` 框架、`common` 序列化模型。

**已确认的设计抉择**（来自上一轮澄清）：
1. `common::PodSpec` 顶层加 `gang`/`topology_constraints`，container `Resource` 加 `gpu`/`gpu_memory`
2. `GpuResources` 用 `requested: u32` 累加（与 CPU/Memory 模型一致），可分配 = `total - requested`
3. `GangStateStore` 纯内存，重启丢弃
4. 新插件默认开启（无 GPU 节点 / 无 gang 注解的 pod 走短路）

**全局约束**：
- 编译耗时长 → 任务粒度内每步只跑「目标 crate 的目标测试」，绝不跑全量 `cargo build`/`cargo test`
- 用 `cargo test --release -p libscheduler --test '<name>' -- --nocapture` 之类的限定调用
- 提交用 `git commit --no-verify`，不带 AI 身份

---

## 任务依赖关系

```
T1 (models 扩展)
  └─→ T2 (GangStateStore)
       └─→ T3 (Cache GPU 维护)
            └─→ T4 (NodeGpuResourcesFit)
                 └─→ T5 (Registry 注入 gang_state)
                      └─→ T6 (TopologyCoAffinityFilter)
                           └─→ T7 (schedule_one Gang 分流)
                                └─→ T8 (后台超时任务)
                                     └─→ T9 (with_xline 解析)
                                          └─→ T10 (E2E 集成测试)
```

每个任务结束都提交一次。

---

## Task 1：扩展数据模型

**目标**：在 `libscheduler::models` 和 `common` 上加 GPU/Gang/Topology 字段，保留现有序列化兼容性。

**Files：**
- Modify: `libscheduler/src/models.rs`
- Modify: `common/src/lib.rs`（PodSpec 顶层 + Resource 加 gpu）

> **实施差异**
> - 给 `common::Resource` 额外加了 `#[derive(Default)]`，让既有的 7 处构造点（`rks/tests/test_garbage_collector.rs`、`rks/tests/test_scheduler.rs`、`libscheduler/tests/xline_test.rs` 等）能用 `..Default::default()` 补齐新字段，最小化非 plan 范围内的改动。
> - 序列化测试改用 `serde_json` 而非 `serde_yaml`：`common` crate 没有 `serde_yaml` 依赖；`serde_json` 已在依赖里，能等价验证 `skip_serializing_if` 与 round-trip。
> - 顺手修了 `libscheduler/tests/xline_test.rs` 的一处既有缺字段问题：`ContainerSpec` 字面量缺 `tty: false`，否则 `cargo check --tests` 通不过。
> - **额外触达文件**（计划未列出，因新字段缺省值要求所有构造点都更新）：
>   - `libscheduler/src/plugins/pod_affinity.rs`（test 内 `make_node`）
>   - `libscheduler/src/with_xline/utils.rs`（占位为 `None`/`0`，Task 9 才真正填值）
>   - `libscheduler/tests/edge_cases.rs`、`libscheduler/tests/test_scheduler.rs`、`libscheduler/tests/xline_test.rs`
>   - `rks/tests/test_garbage_collector.rs`、`rks/tests/test_scheduler.rs`

### Step 1.1：在 `libscheduler/src/models.rs` 加新结构

在 `ResourcesRequirements` 下面追加：

```rust
#[derive(Clone, Default, Debug)]
pub struct GpuResources {
    pub total: u32,
    pub requested: u32,
    pub memory_per_gpu: u64,
    pub model: String,
}

#[derive(Clone, Default, Debug)]
pub struct GangSpec {
    pub id: String,
    pub size: u32,
}

#[derive(Clone, Default, Debug)]
pub struct TopologyConstraint {
    pub topology_key: String,
    /// MVP only supports same_value=true (co-affinity).
    pub same_value: bool,
}
```

### Step 1.2：扩展 `PodSpec` / `NodeInfo`

```rust
pub struct PodSpec {
    // existing fields unchanged...
    pub gpu_request: u32,
    pub gpu_memory_request: Option<u64>,
    pub gang: Option<GangSpec>,
    pub topology_constraints: Vec<TopologyConstraint>,
}

pub struct NodeInfo {
    // existing fields unchanged...
    pub gpu_resources: Option<GpuResources>,
}
```

### Step 1.3：在 `common/src/lib.rs` `PodSpec` 加字段

```rust
pub struct PodSpec {
    // existing fields unchanged...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gang: Option<GangSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topology_constraints: Vec<TopologyConstraint>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct GangSpec {
    pub id: String,
    pub size: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TopologyConstraint {
    pub topology_key: String,
    #[serde(default)]
    pub same_value: bool,
}
```

容器资源 `Resource` 增加可选字段：

```rust
pub struct Resource {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory: Option<String>, // e.g. "80Gi"
}
```

### Step 1.4：写测试验证序列化往返

新增 `libscheduler/src/models.rs` `#[cfg(test)] mod tests`：

```rust
#[test]
fn pod_spec_default_has_no_gang() {
    let s = PodSpec::default();
    assert!(s.gang.is_none());
    assert_eq!(s.gpu_request, 0);
    assert!(s.topology_constraints.is_empty());
}
```

新增 `common/src/lib.rs` 测试：

```rust
#[test]
fn pod_spec_yaml_omits_gang_when_absent() {
    let s = PodSpec::default();
    let y = serde_yaml::to_string(&s).unwrap();
    assert!(!y.contains("gang"));
}

#[test]
fn pod_spec_yaml_roundtrip_with_gang() {
    let mut s = PodSpec::default();
    s.gang = Some(GangSpec { id: "g1".into(), size: 4 });
    s.topology_constraints.push(TopologyConstraint {
        topology_key: "topology.rk8s.io/nvlink-domain".into(),
        same_value: true,
    });
    let y = serde_yaml::to_string(&s).unwrap();
    let back: PodSpec = serde_yaml::from_str(&y).unwrap();
    assert_eq!(back.gang.as_ref().unwrap().size, 4);
    assert_eq!(back.topology_constraints.len(), 1);
}
```

**Step 1.5：跑测试**

```bash
cargo test --release -p libscheduler models:: -- --nocapture
cargo test --release -p common pod_spec_ -- --nocapture
```

期望：新增测试 PASS，无回归。

**Step 1.6：提交**

```bash
git add libscheduler/src/models.rs common/src/lib.rs
git commit --no-verify -m "feat(libscheduler): add GPU/Gang/Topology fields to data model"
```

---

## Task 2：GangStateStore 跨周期共享状态

**目标**：新建 `libscheduler/src/gang_state.rs`，提供线程安全的 Gang 注册表。

**Files：**
- Create: `libscheduler/src/gang_state.rs`
- Modify: `libscheduler/src/lib.rs`（暴露模块）

> **实施差异（重要）**
> - 实际实现**直接采用 `std::sync::RwLock` + 同步方法**，不再先写 `tokio::sync::RwLock` 再回退（Task 6 的「修改决议」前置到 Task 2 执行，避免一次重写）。
> - 因此 Step 2.1 的测试不是 `#[tokio::test]` 异步形式，而是普通同步 `#[test]`，不再 `await`。
> - 锁类型：`Arc<RwLock<HashMap<String, GangEntry>>>`（`std::sync::RwLock`）。`add_member`/`take_and_clear`/`assumed_nodes`/`collect_timed_out` 全部为同步方法。
> - **新增方法 `take_timed_out(timeout)`**（计划未列）：原子地「检测超时 + 移除条目」一次完成，返回 `Vec<(gang_id, members)>`。这是为了消除 Task 8 watchdog 中「先 `collect_timed_out` 再 `take_and_clear`」存在的 race（在两次调用之间 gang 可能恰好被 `schedule_one` 凑齐，导致重复消费成员）。

### Step 2.1：写失败测试（先写测试再写实现）

`libscheduler/src/gang_state.rs` 顶部草稿（先写 test 模块）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn add_member_until_full_returns_true_on_last() {
        let store = GangStateStore::default();
        assert!(!store.add_member("g1", 3, "p1", "n1").await);
        assert!(!store.add_member("g1", 3, "p2", "n1").await);
        assert!(store.add_member("g1", 3, "p3", "n1").await);
    }

    #[tokio::test]
    async fn take_and_clear_returns_all_members() {
        let store = GangStateStore::default();
        store.add_member("g1", 2, "p1", "n1").await;
        store.add_member("g1", 2, "p2", "n2").await;
        let members = store.take_and_clear("g1").await;
        assert_eq!(members.unwrap().len(), 2);
        assert!(store.take_and_clear("g1").await.is_none());
    }

    #[tokio::test]
    async fn collect_timed_out_returns_only_old_entries() {
        let store = GangStateStore::default();
        store.add_member("g_old", 4, "p1", "n1").await;
        // simulate created_at being old by directly mutating
        store.set_created_at_for_test("g_old", Instant::now() - Duration::from_secs(1000)).await;
        store.add_member("g_new", 4, "p1", "n1").await;
        let timed_out = store.collect_timed_out(Duration::from_secs(60)).await;
        assert_eq!(timed_out, vec!["g_old".to_string()]);
    }

    #[tokio::test]
    async fn topology_neighbors_returns_assumed_node_names() {
        let store = GangStateStore::default();
        store.add_member("g1", 4, "p1", "node-a").await;
        store.add_member("g1", 4, "p2", "node-b").await;
        let neighbors = store.assumed_nodes("g1").await;
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.iter().any(|n| n == "node-a"));
    }
}
```

### Step 2.2：实现

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

#[derive(Debug)]
pub struct GangEntry {
    pub size: u32,
    pub assumed: HashMap<String, String>, // pod_name -> node_name
    pub created_at: Instant,
}

#[derive(Default, Clone)]
pub struct GangStateStore {
    inner: Arc<RwLock<HashMap<String, GangEntry>>>,
}

impl GangStateStore {
    /// Returns true if this membership filled the gang to its declared size.
    pub async fn add_member(&self, gang_id: &str, size: u32, pod: &str, node: &str) -> bool {
        let mut g = self.inner.write().await;
        let entry = g.entry(gang_id.to_string()).or_insert_with(|| GangEntry {
            size,
            assumed: HashMap::new(),
            created_at: Instant::now(),
        });
        entry.assumed.insert(pod.to_string(), node.to_string());
        entry.assumed.len() as u32 >= entry.size
    }

    pub async fn take_and_clear(&self, gang_id: &str) -> Option<HashMap<String, String>> {
        let mut g = self.inner.write().await;
        g.remove(gang_id).map(|e| e.assumed)
    }

    pub async fn assumed_nodes(&self, gang_id: &str) -> Vec<String> {
        let g = self.inner.read().await;
        g.get(gang_id)
            .map(|e| e.assumed.values().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn collect_timed_out(&self, timeout: Duration) -> Vec<String> {
        let g = self.inner.read().await;
        let now = Instant::now();
        g.iter()
            .filter(|(_, e)| {
                (e.assumed.len() as u32) < e.size && now.duration_since(e.created_at) > timeout
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    #[cfg(test)]
    pub async fn set_created_at_for_test(&self, gang_id: &str, ts: Instant) {
        let mut g = self.inner.write().await;
        if let Some(e) = g.get_mut(gang_id) {
            e.created_at = ts;
        }
    }
}
```

### Step 2.3：在 `lib.rs` 暴露

```rust
pub mod gang_state;
```

### Step 2.4：跑测试 + 提交

```bash
cargo test --release -p libscheduler gang_state:: -- --nocapture
git add libscheduler/src/gang_state.rs libscheduler/src/lib.rs
git commit --no-verify -m "feat(libscheduler): add GangStateStore for cross-cycle gang membership"
```

---

## Task 3：Cache GPU 资源维护

**目标**：`Cache::assume`/`unassume`/`remove_pod` 维护 `node.gpu_resources.requested`。

**Files：**
- Modify: `libscheduler/src/cache.rs`

> **实施差异**
> - **`assume` 加了幂等判断**（落实「风险与缓解」表中 `assume` 重复扣减的提示）：进入函数后先检查 `pod.scheduled == Some(node)`，若已是同一节点则直接返回 `true`，不再二次累加 cpu/memory/gpu requested。
> - **`update_node` 同步保留 `gpu_resources.requested`**：原代码已对 `requested.cpu/memory` 做了「跨更新保留」处理，现把 `gpu_resources.requested` 也纳入同样的保留逻辑（用 `new_node.gpu_resources` 的 total/memory_per_gpu/model 覆盖，但 requested 沿用旧值）。该处 plan 未点名，但属于必要补完。
> - 单测多加了两条：`assume_is_idempotent`、`update_node_preserves_gpu_requested`。

### Step 3.1：先写失败测试

在 `cache.rs` 末尾 `#[cfg(test)] mod tests`（如不存在则新建）追加：

```rust
#[test]
fn assume_increments_gpu_requested() {
    let mut cache = Cache::new();
    let mut node = NodeInfo::default();
    node.name = "n1".into();
    node.gpu_resources = Some(GpuResources { total: 8, requested: 0, memory_per_gpu: 0, model: String::new() });
    cache.update_node(node);

    let mut pod = PodInfo::default();
    pod.name = "p1".into();
    pod.spec.gpu_request = 4;
    cache.update_pod(pod);

    assert!(cache.assume("p1", "n1"));
    let n = cache.get_nodes().into_iter().find(|n| n.name == "n1").unwrap();
    assert_eq!(n.gpu_resources.unwrap().requested, 4);
}

#[test]
fn unassume_decrements_gpu_requested() {
    // ... mirror, then unassume, expect requested back to 0
}

#[test]
fn remove_pod_releases_gpu_count() {
    // ... assume, remove_pod, expect requested back to 0
}
```

### Step 3.2：在三处 mut node 时改 GPU 计数

```rust
// in assume()
if let (Some(node_gpu), req) = (node.gpu_resources.as_mut(), pod_info.spec.gpu_request)
    && req > 0
{
    node_gpu.requested = node_gpu.requested.saturating_add(req);
}

// in unassume()
if let (Some(node_gpu), req) = (node.gpu_resources.as_mut(), pod_info.spec.gpu_request)
    && req > 0
{
    node_gpu.requested = node_gpu.requested.saturating_sub(req);
}

// in remove_pod() (the existing if-chain that decreases cpu/memory)
if let Some(node_gpu) = node.gpu_resources.as_mut()
    && p.spec.gpu_request > 0
{
    node_gpu.requested = node_gpu.requested.saturating_sub(p.spec.gpu_request);
}
```

### Step 3.3：跑测试 + 提交

```bash
cargo test --release -p libscheduler cache:: -- --nocapture
git add libscheduler/src/cache.rs
git commit --no-verify -m "feat(libscheduler): track GPU requested count in Cache assume/unassume"
```

---

## Task 4：NodeGpuResourcesFit 插件

**目标**：参照 `node_resources_fit.rs` 写 GPU 维度过滤 + 评分插件，未声明 `gpu_request` 的 pod 直接 `Skip`。

**Files：**
- Create: `libscheduler/src/plugins/node_gpu_resources_fit.rs`
- Modify: `libscheduler/src/plugins/mod.rs`（暴露）

> **实施差异**
> - 评分公式实际写为 `score = (free_after) * 100 / total`（free_after = 节点剩余 GPU 数预估），最大化空闲 GPU 数即等价 LeastAllocated；plan 中给的注释 `100 * (total - requested - pod_request) / total` 等价。
> - `pre_score` 在 `gpu_request == 0` 时也返回 `Skip`（plan 只点了 `pre_filter`，但 `score` 同样需要 `pre_score` 状态，`Skip` 让评分相位干净跳过）。
> - 节点没有 `gpu_resources` 字段时（CPU-only 节点）：filter 直接返回 `Unschedulable`，score 返回 `0` + Success（不否决，但不加分）。
> - `EnqueueExtension` hint：`Pod Delete` → `Queue`（释放 GPU 可能让本 pod 可调度），`Node Add | UpdateNodeAllocatable` → 用 `is_gpu_fit` 判断后 `Queue`/`Skip`。

### Step 4.1：先写测试草稿

放在新文件底部 `#[cfg(test)]`：

- pod 无 `gpu_request` → `pre_filter` 返回 `Code::Skip`
- pod gpu_request=4，节点 total=2 → `filter` 返回 `Unschedulable`
- pod gpu_request=2，节点 total=8/requested=4 → `filter` 返回 `Success`
- pod gpu_memory_request=80GiB，节点 memory_per_gpu=40GiB → `filter` 返回 `Unschedulable`
- 评分：节点 A 剩余 6 GPU、节点 B 剩余 2 GPU → A 得分高于 B（LeastAllocated）

### Step 4.2：实现

骨架（细节按 `node_resources_fit.rs` 模仿）：

```rust
pub struct NodeGpuResourcesFit;

impl Plugin for NodeGpuResourcesFit {
    fn name(&self) -> &str { "NodeGpuResourcesFit" }
}

struct GpuPreFilterState {
    gpu_request: u32,
    gpu_memory_request: Option<u64>,
}

const GPU_PREFILTER_KEY: &str = "PreFilterNodeGpuResourcesFit";

impl PreFilterPlugin for NodeGpuResourcesFit {
    fn pre_filter(&self, state: &mut CycleState, pod: &PodInfo, _: Vec<NodeInfo>)
        -> (PreFilterResult, Status)
    {
        if pod.spec.gpu_request == 0 {
            return (PreFilterResult { node_names: vec![] },
                    Status { code: Code::Skip, ..Default::default() });
        }
        state.write(GPU_PREFILTER_KEY, Box::new(GpuPreFilterState {
            gpu_request: pod.spec.gpu_request,
            gpu_memory_request: pod.spec.gpu_memory_request,
        }));
        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

impl FilterPlugin for NodeGpuResourcesFit {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node: NodeInfo) -> Status {
        let s = match state.read::<GpuPreFilterState>(GPU_PREFILTER_KEY) {
            Some(s) => s,
            None => return Status::default(), // nothing to enforce
        };
        let gpu = match &node.gpu_resources {
            Some(g) => g,
            None => return Status::new(Code::Unschedulable,
                vec!["node has no GPU resources".into()]),
        };
        let avail = gpu.total.saturating_sub(gpu.requested);
        if avail < s.gpu_request {
            return Status::new(Code::Unschedulable,
                vec!["insufficient GPU count".into()]);
        }
        if let Some(req_mem) = s.gpu_memory_request
            && req_mem > gpu.memory_per_gpu
        {
            return Status::new(Code::Unschedulable,
                vec!["insufficient GPU memory per card".into()]);
        }
        Status::default()
    }
}

impl PreScorePlugin for NodeGpuResourcesFit { /* write same state */ }

impl ScorePlugin for NodeGpuResourcesFit {
    // LeastAllocated: 100 * (total - requested - pod_request) / total
    // Or: prefer nodes with more free GPUs after assignment.
}
```

`EnqueueExtension` 复用 `Pod Delete + Node Add/UpdateAllocatable` 两个 hint。

### Step 4.3：在 `plugins/mod.rs` 注册

- `pub mod node_gpu_resources_fit;`
- `Registry::default()` 与 `Plugins::default()` 加：
  - `pre_filter`/`filter`/`pre_score`/`score`/`enqueue_extensions` 都加 `NodeGpuResourcesFit`，weight 1

### Step 4.4：跑测试 + 提交

```bash
cargo test --release -p libscheduler node_gpu_resources_fit -- --nocapture
git add libscheduler/src/plugins/node_gpu_resources_fit.rs libscheduler/src/plugins/mod.rs
git commit --no-verify -m "feat(libscheduler): add NodeGpuResourcesFit plugin"
```

---

## Task 5：Registry/Plugins 注入 GangStateStore

**目标**：`Registry::default()` → `Registry::new(gang_state)`；`Scheduler::new` 接受/创建 `gang_state` 并向 `Registry` 注入。`TopologyCoAffinityFilter`（下一任务）会需要它。

**Files：**
- Modify: `libscheduler/src/plugins/mod.rs`
- Modify: `libscheduler/src/scheduler.rs`

> **实施差异**
> - 与 Task 4、Task 6 合并到同一次提交（`f89d9abd8`）：因为 `Registry::new` 必须同时知道两个新插件的存在才能编译，三个 task 强耦合。
> - `Plugins::default()` 中也新增了 `gpu_fit`（NodeGpuResourcesFit）和 `topology_coaffinity` 两个 `PluginInfo`，覆盖 `pre_filter` / `filter` / `pre_score` / `score` / `enqueue_extensions` 五个相位（topology 只参与 `pre_filter`/`filter`）。
> - `Scheduler` 结构体新增字段 `gang_state: Arc<GangStateStore>`，在 `Scheduler::new` 内部 `Arc::new(GangStateStore::default())` 并复用给 `Registry::new`，保持 `Default for Scheduler` 行为不变。

### Step 5.1：改造 Registry

```rust
impl Registry {
    pub fn new(gang_state: Arc<GangStateStore>) -> Self {
        // ...同 default()，但 topology_coaffinity 用 gang_state 构造
        // 现阶段先把 gang_state 字段传进来但不使用（占位），等 Task 6 用上
    }
}

// 保留 Default，内部调 Self::new(Arc::new(GangStateStore::default()))
impl Default for Registry { fn default() -> Self { Self::new(Arc::new(GangStateStore::default())) } }
```

### Step 5.2：改造 Scheduler

```rust
pub struct Scheduler {
    cache: Arc<RwLock<Cache>>,
    queue: Arc<SchedulingQueue>,
    strategy: ScoringStrategy,
    enabled_plugins: EnabledPlugins,
    gang_state: Arc<GangStateStore>,   // ← new
}

impl Scheduler {
    pub fn new(strategy: ScoringStrategy, plugins: Plugins) -> Self {
        let gang_state = Arc::new(GangStateStore::default());
        let registry = Registry::new(gang_state.clone());
        // ... rest unchanged, set self.gang_state = gang_state
    }
}
```

### Step 5.3：测试

现有 `test_plugins_enabled` / `test_schedule_one_assigns_pod` 不破坏即可：

```bash
cargo test --release -p libscheduler scheduler:: -- --nocapture
git add libscheduler/src/plugins/mod.rs libscheduler/src/scheduler.rs
git commit --no-verify -m "refactor(libscheduler): inject GangStateStore into Registry/Scheduler"
```

---

## Task 6：TopologyCoAffinityFilter 插件

**目标**：跨调度周期约束：Gang 内第 N 个 Pod (N>1) 只能落在与已 assume 成员相同 `topology_key` 值的节点上。

**Files：**
- Create: `libscheduler/src/plugins/topology_coaffinity.rs`
- Modify: `libscheduler/src/plugins/mod.rs`

> **实施差异**
> - **没有用 `futures::executor::block_on`**：因为 Task 2 直接采用同步 `GangStateStore`，pre_filter 里直接调 `self.gang_state.assumed_nodes(&gang.id)`，不需要 `block_on`，也不需要新增 `futures` 依赖。原计划「方案 A」与「修改决议」里的折中已经被前置成最终方案。
> - `PreFilterCtx` 简化为只保存 `required_values: HashMap<String, String>`：原计划字段 `gang_id`/`constraints` 在 filter 阶段并不需要，去掉。
> - `pick_label_value` 实现：遍历 assumed 节点名，在传入的 nodes snapshot 里找到第一个有该 label 的节点取值；若 assumed 节点全部缺失该 label，返回 `Unschedulable`（plan 中的 `missing_topology_label_on_assumed_node_treats_as_unschedulable` 测试覆盖）。

### Step 6.1：写测试草稿

```rust
#[tokio::test]
async fn first_pod_no_constraint() {
    // pod with gang+topology_constraint, no members yet → Filter passes any node
}

#[tokio::test]
async fn second_pod_constrained_to_first_member_topology() {
    // gang_state has p1 → node-a (label nvlink-domain=domain0)
    // node-a passes; node-b (domain1) rejected
}

#[tokio::test]
async fn pod_without_gang_short_circuits_skip() {
    // pod has no gang → pre_filter returns Skip
}

#[tokio::test]
async fn missing_topology_label_on_assumed_node_treats_as_unschedulable() {
    // p1 already assumed on a node lacking the label → all subsequent fail
}
```

### Step 6.2：实现

```rust
pub struct TopologyCoAffinityFilter {
    pub gang_state: Arc<GangStateStore>,
}

impl Plugin for TopologyCoAffinityFilter {
    fn name(&self) -> &str { "TopologyCoAffinityFilter" }
}

const STATE_KEY: &str = "PreFilterTopologyCoAffinity";

struct PreFilterCtx {
    gang_id: String,
    constraints: Vec<TopologyConstraint>,
    /// label key -> required value (for each constraint with same_value=true)
    required_values: HashMap<String, String>,
}

impl PreFilterPlugin for TopologyCoAffinityFilter {
    fn pre_filter(&self, state, pod, nodes) -> (PreFilterResult, Status) {
        let Some(gang) = pod.spec.gang.as_ref() else {
            return (PreFilterResult { node_names: vec![] },
                    Status { code: Code::Skip, ..Default::default() });
        };
        if pod.spec.topology_constraints.is_empty() {
            return (PreFilterResult { node_names: vec![] },
                    Status { code: Code::Skip, ..Default::default() });
        }

        // Synchronous read into the runtime is awkward. We use the cycle's tokio context:
        let assumed = futures::executor::block_on(self.gang_state.assumed_nodes(&gang.id));
        let mut required = HashMap::new();
        if !assumed.is_empty() {
            // For each topology constraint with same_value=true, look up the label value
            // on any of the assumed nodes (they should already agree).
            for c in &pod.spec.topology_constraints {
                if !c.same_value { continue; }
                if let Some(v) = pick_label_value(&assumed, &c.topology_key, &nodes) {
                    required.insert(c.topology_key.clone(), v);
                } else {
                    return (PreFilterResult { node_names: vec![] },
                            Status::new(Code::Unschedulable,
                                vec!["assumed gang member missing topology label".into()]));
                }
            }
        }

        state.write(STATE_KEY, Box::new(PreFilterCtx {
            gang_id: gang.id.clone(),
            constraints: pod.spec.topology_constraints.clone(),
            required_values: required,
        }));
        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

impl FilterPlugin for TopologyCoAffinityFilter {
    fn filter(&self, state, _pod, node) -> Status {
        let Some(ctx) = state.read::<PreFilterCtx>(STATE_KEY) else { return Status::default() };
        if ctx.required_values.is_empty() { return Status::default(); }
        for (k, v) in &ctx.required_values {
            match node.labels.get(k) {
                Some(nv) if nv == v => {}
                _ => return Status::new(Code::Unschedulable,
                    vec![format!("node label {k} mismatch for gang topology")]),
            }
        }
        Status::default()
    }
}
```

注：`pick_label_value` 内部按节点名在 `nodes` 里找匹配节点取 label。

**关于 `block_on`**：scheduler 主循环里 prefilter 是同步函数；`gang_state` 用 tokio RwLock。我们暂定方案 A：在 `GangStateStore` 上提供同步访问方法（用 `std::sync::RwLock` 替代 `tokio::sync::RwLock`）。这避免了在同步 prefilter 中嵌入 `block_on`。

→ **修改决议**：把 Task 2 中的 `tokio::sync::RwLock` 改成 `std::sync::RwLock`，方法改回同步。Task 7 主循环中读写改成同步调用即可（持锁时间短，远小于跨 await 的开销）。

### Step 6.3：注册 + 测试 + 提交

`mod.rs` 中的 `Registry::new` 用 `gang_state` 构造 `TopologyCoAffinityFilter`，加到 `pre_filter`/`filter` 列表。

```bash
cargo test --release -p libscheduler topology_coaffinity -- --nocapture
git add libscheduler/src/plugins/topology_coaffinity.rs libscheduler/src/plugins/mod.rs libscheduler/src/gang_state.rs
git commit --no-verify -m "feat(libscheduler): add TopologyCoAffinityFilter plugin"
```

---

## Task 7：schedule_one Gang 分流

**目标**：`schedule_one` 末尾在 assume 成功后判断 gang，不齐则不发 Assignment，齐了一次性发完。

**Files：**
- Modify: `libscheduler/src/scheduler.rs`

> **实施差异**
> - `schedule_one` 增加 `gang_state: Arc<GangStateStore>` 参数；`Scheduler::run` 闭包以及现有的 `test_schedule_one_assigns_pod` 测试同步更新（传入 `scheduler.gang_state.clone()`）。
> - 新增单测 `test_schedule_one_gang_holds_until_full`：构造 3 个同 gang 的 pod、`size=3`，前两次 `schedule_one` 调用断言 `rx.try_recv().is_err()`，第三次断言三条 Assignment 一次性下发。

### Step 7.1：测试

新增集成测试：4 个带相同 `gang_id`、`size=4` 的 pod，依次入队；前三次调度后 `rx` 没消息；第四次调度后 `rx` 收到恰好 4 条 Assignment。

### Step 7.2：改 `schedule_one` 末尾逻辑

把 `gang_state: Arc<GangStateStore>` 加进 `schedule_one` 参数列表（也加进 `Scheduler::run` spawn 的闭包）。

```rust
let mut cache_write = cache.write().await;
let chosen = scores[0].1.name.clone();
if cache_write.assume(&pod_name, &chosen) {
    drop(cache_write);
    match pod_info.spec.gang.as_ref() {
        None => {
            res_sx.send(Ok(Assignment { pod_name, node_name: chosen }))
                .expect("scheduling result rx closed");
        }
        Some(g) => {
            let full = gang_state.add_member(&g.id, g.size, &pod_name, &chosen);
            if full {
                if let Some(members) = gang_state.take_and_clear(&g.id) {
                    for (p, n) in members {
                        res_sx.send(Ok(Assignment { pod_name: p, node_name: n }))
                            .expect("scheduling result rx closed");
                    }
                }
            }
            // not full → silent return; pod stays "assumed but not bound"
        }
    }
}
```

### Step 7.3：测试 + 提交

```bash
cargo test --release -p libscheduler scheduler::tests -- --nocapture
git add libscheduler/src/scheduler.rs
git commit --no-verify -m "feat(libscheduler): defer Assignment send until full gang assumed"
```

---

## Task 8：Gang 超时回滚后台任务

**目标**：`Scheduler::run` 启动一个 ticker（默认 1s 轮询），找出 `created_at` 超过阈值（默认 60s，常量提供）仍未凑齐的 gang，调 `Cache::unassume` 回滚，并把回滚后的 pod 重新推回 active queue。

**Files：**
- Modify: `libscheduler/src/scheduler.rs`

> **实施差异**
> - 常量改为 `pub const GANG_TIMEOUT_DEFAULT` 和 `pub const GANG_TIMEOUT_CHECK_INTERVAL_DEFAULT`，并在 `Scheduler` 上加字段 `gang_timeout` / `gang_timeout_check_interval` + 测试 hook `with_gang_timeout(timeout, check_interval) -> Self`，落实「风险与缓解」表的最后一条。
> - **使用 `take_timed_out` 而不是 `collect_timed_out + take_and_clear`**：见 Task 2 实施差异——单次原子操作消除 race。
> - 回滚顺序：先在 `cache.write()` 锁内部对每个超时 gang 的成员逐个 `unassume`，把要重排的 `(name, priority)` 收到 `to_requeue` 里；释放 cache 锁后再调 `queue.push(...)`，避免在持 cache 锁时与 queue 锁形成嵌套等待。

### Step 8.1：常量与配置

```rust
const GANG_TIMEOUT: Duration = Duration::from_secs(60);
const GANG_TIMEOUT_CHECK_INTERVAL: Duration = Duration::from_secs(1);
```

### Step 8.2：在 `run()` 里加后台任务

```rust
let gang_state = self.gang_state.clone();
let cache = self.cache.clone();
let queue = self.queue.clone();
tokio::spawn(async move {
    let mut t = interval(GANG_TIMEOUT_CHECK_INTERVAL);
    loop {
        t.tick().await;
        let timed_out = gang_state.collect_timed_out(GANG_TIMEOUT);
        for gid in timed_out {
            if let Some(members) = gang_state.take_and_clear(&gid) {
                let mut cache_w = cache.write().await;
                for pod_name in members.keys() {
                    if let Some(p) = cache_w.unassume(pod_name) {
                        // re-enqueue
                        // (drop write lock before pushing to queue if needed)
                    }
                }
                drop(cache_w);
                // separately push to queue
            }
        }
    }
});
```

### Step 8.3：测试

集成测试：3/4 个成员入队（第 4 个 pod 故意不投递）→ 等待 > timeout → 验证前 3 个 pod 在 cache 里 `scheduled = None`，且重新出现在 active_queue（断言 `next_pod()` 拿得到）。

为避免实际等 60s，测试里把 `GANG_TIMEOUT`/`GANG_TIMEOUT_CHECK_INTERVAL` 暴露为可注入参数，或在 `Scheduler` 加 `with_gang_timeout(...)` 测试 hook。

### Step 8.4：提交

```bash
cargo test --release -p libscheduler gang_timeout -- --nocapture
git add libscheduler/src/scheduler.rs
git commit --no-verify -m "feat(libscheduler): rollback timed-out gang assumptions"
```

---

## Task 9：with_xline 解析新字段

**目标**：YAML 中 container 的 `gpu`/`gpu_memory` 被汇总到 `pod_info.spec.gpu_request`/`gpu_memory_request`；pod 的 `gang`/`topology_constraints` 直接透传；node 的 `nvidia.com/gpu.count` 等 label 解析成 `NodeInfo.gpu_resources`。

**Files：**
- Modify: `libscheduler/src/with_xline/utils.rs`

> **实施差异**
> - **init_containers 也参与汇总**：与现有 cpu/memory 的语义一致——`init_containers` 取 `max`，再与 `containers` 的累加值取 `max`。GPU 数量按 `init_gpu = max(init_containers.gpu)` → `total_gpu = max(total_gpu_from_containers, init_gpu)`；GPU memory 同理。Plan 草稿只展示了 `containers` 的循环，这里补全了 init 路径。
> - 单元测试用了三段嵌入的 YAML 字符串：`parse_pod_with_gpu_and_gang`、`parse_node_with_gpu_labels`、`parse_node_without_gpu_labels`（最后一条额外覆盖「无 GPU 节点 → `gpu_resources == None`」）。
> - 由于 Task 1 提交时此处先用占位值 `gpu_request: 0` / `gpu_resources: None` 编译，本任务的实质工作是把占位换成真正解析逻辑。

### Step 9.1：补 `convert_pod_task_to_pod_info`

```rust
let mut total_gpu = 0u32;
let mut max_gpu_mem: Option<u64> = None;
for c in &pod_task.spec.containers {
    if let Some(res) = &c.resources
        && let Some(lim) = &res.limits
    {
        total_gpu += lim.gpu.unwrap_or(0);
        if let Some(s) = &lim.gpu_memory {
            let v = parse_memory(s);
            max_gpu_mem = Some(max_gpu_mem.map_or(v, |m| m.max(v)));
        }
    }
}

let spec = PodSpec {
    // existing...
    gpu_request: total_gpu,
    gpu_memory_request: max_gpu_mem,
    gang: pod_task.spec.gang.map(|g| crate::models::GangSpec { id: g.id, size: g.size }),
    topology_constraints: pod_task.spec.topology_constraints.into_iter()
        .map(|c| crate::models::TopologyConstraint { topology_key: c.topology_key, same_value: c.same_value })
        .collect(),
};
```

### Step 9.2：补 `convert_k8s_node_to_node_info`

```rust
let gpu_resources = labels.get("nvidia.com/gpu.count")
    .and_then(|v| v.parse::<u32>().ok())
    .map(|total| GpuResources {
        total,
        requested: 0,
        memory_per_gpu: labels.get("nvidia.com/gpu.memory-gib")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|gib| gib * 1024 * 1024 * 1024)
            .unwrap_or(0),
        model: labels.get("nvidia.com/gpu.product").cloned().unwrap_or_default(),
    });
```

### Step 9.3：单元测试

直接构造 YAML 字符串 → `serde_yaml` → 调 convert，断言 `gpu_request == 4`、`gang.size == 4`、`gpu_resources.unwrap().total == 8`。

### Step 9.4：提交

```bash
cargo test --release -p libscheduler with_xline -- --nocapture
git add libscheduler/src/with_xline/utils.rs
git commit --no-verify -m "feat(libscheduler): parse GPU/Gang/Topology fields from xline payloads"
```

---

## Task 10：端到端集成测试

**目标**：综合验证完整链路（不依赖 Xline）：构造 4 个 pod + 2 个 GPU 节点 → 跑 `Scheduler::run` → 期望 4 条 Assignment 全部指向同一 nvlink-domain 内的节点。

**Files：**
- Create: `libscheduler/tests/llm_scheduler_e2e.rs`

> **实施差异**
> - 第一条用例 `gang_with_topology_lands_on_same_domain`：node-A 8 GPU domain0、node-B 4 GPU domain1，4 个 pod 各请求 2 GPU + gang.size=4，预期全部落到 node-A。
> - 第二条用例 `gang_timeout_rolls_back_when_unable_to_satisfy`：用 `with_gang_timeout(200ms, 50ms)` 把超时缩到亚秒级（避免实际等 60s）；只给一个 4 GPU 的 node，gang 永远凑不齐 → 断言 2s 内 `rx.recv()` 始终 `Err`（即没有任何 Assignment 被下发）。
> - **每个 pod 也声明了非零 cpu/memory 请求**，避免被 `NodeResourcesFit` 评分函数当作零请求节点（保持评分相位的可观测性）。

### Step 10.1：测试草稿

```rust
#[tokio::test]
async fn gang_with_topology_lands_on_same_domain() {
    let scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    // node-A: 8 GPU, nvlink-domain=domain0
    // node-B: 4 GPU, nvlink-domain=domain1
    // 4 pods: gpu_request=2, gang.size=4, topology_constraint nvlink-domain
    // ...
    let mut rx = scheduler.run();
    for pod in pods { scheduler.enqueue(pod).await; }
    let mut got = vec![];
    for _ in 0..4 {
        let a = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap().unwrap();
        got.push(a);
    }
    // Assert all node_name == "node-A" (only domain that satisfies size 4 with 2 GPU each = 8 GPU needed)
    assert!(got.iter().all(|a| a.node_name == "node-A"));
}

#[tokio::test]
async fn gang_timeout_rolls_back_when_unable_to_satisfy() {
    // Only 1 GPU node with 2 GPU; gang size 4, gpu_request=2 each
    // first pod assumes, others fail → timeout → first pod unassumed, all back in queue
}
```

### Step 10.2：提交

```bash
cargo test --release -p libscheduler --test llm_scheduler_e2e -- --nocapture
git add libscheduler/tests/llm_scheduler_e2e.rs
git commit --no-verify -m "test(libscheduler): add LLM inference E2E scheduling test"
```

---

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| `tokio::sync::RwLock` 在同步插件里不能 `await` | Task 6 决议改为 `std::sync::RwLock`，所有 GangStateStore 方法同步 |
| 改 `schedule_one` 签名导致测试不匹配 | 现有测试 `test_schedule_one_assigns_pod` 直接调函数，改签名时同步更新（加 `Arc::new(GangStateStore::default())` 参数） |
| Cache `assume` 调多次（`update_cache_pod` 重放 + `schedule_one` 自身）导致 GPU 重复扣减 | `assume` 已有 idempotent 检查的语义吗？需要核对：当前实现没有，可能存在重复扣减——Task 3 同时给 `assume` 加幂等保护：若 `pod.scheduled.is_some()` 已是同一个 node 则直接 return true 不重复扣。 |
| `with_xline` 中 `gang.size` mismatch | MVP first-write-wins，不做校验 |
| 后台超时任务测试要等 60s | 暴露 `Scheduler::with_gang_timeout(d)` 测试 hook |

---

## 总览

10 个任务，预计每个 30~90 分钟，总工作量约 1~1.5 个工作日。
单元测试覆盖每个新数据结构与插件，集成测试覆盖端到端 gang+topology 流程。
所有提交带 `--no-verify`，不引入 AI 身份。

---

## 实施差异总览（2026-05-07 落地）

> 与原计划相比的主要偏离点。详细记录见各 Task 内的「实施差异」框。

### 1. 锁与并发模型
- **`GangStateStore` 自始至终使用 `std::sync::RwLock` + 同步方法**：把 Task 6 的「修改决议」前置到 Task 2，避免一次返工。
- 新增方法 `take_timed_out(timeout)`：原子地完成「检测超时 + 移除条目」。`collect_timed_out` 仍保留（作为只读探查），但 watchdog 改用 `take_timed_out` 以消除 race。

### 2. Cache 行为
- `assume` 加了幂等保护：若 pod 已 assume 在同一节点上，直接返回 `true` 不再二次扣资源。落实「风险与缓解」表中 `update_cache_pod` 重放导致重复扣减的隐患。
- `update_node` 同步保留 `gpu_resources.requested`（与 cpu/memory `requested` 跨更新保留一致）。

### 3. 共序列化与依赖
- `common::Resource` 增加 `#[derive(Default)]`，让既有构造点能用 `..Default::default()` 增量补字段。
- 测试用 `serde_json` 而非 `serde_yaml`（`common` crate 没有 `serde_yaml` 依赖；功能等价）。
- 顺手修了 `libscheduler/tests/xline_test.rs` 一处既有 `tty: false` 缺失。

### 4. 任务边界
- **Task 4 / 5 / 6 合并为单次提交**（`f89d9abd8`）：`Registry::new` 的签名变更必须同时知晓两个新插件，三者强耦合，分开提交反而无法编译。
- Task 1 提交时，`with_xline/utils.rs` 中的 GPU/Gang 字段先用占位值（0/None）让代码能 `cargo check` 通过；Task 9 才真正实现解析。

### 5. 调度循环
- `schedule_one` 函数签名增加 `gang_state: Arc<GangStateStore>` 参数，`Scheduler::run` 闭包以及现有测试 `test_schedule_one_assigns_pod` 同步更新。
- `Scheduler::run` 在原有的 active-queue 调度协程之外，新增第二个协程：gang timeout watchdog（按 `gang_timeout_check_interval` 心跳，调 `take_timed_out` → cache.unassume → queue.push）。
- `Scheduler` 加测试 hook `with_gang_timeout(timeout, check_interval) -> Self`（builder 风格），E2E 测试用它把超时缩到 200ms。

### 6. with_xline 解析
- init_containers 也参与 GPU/GPU memory 汇总（与 cpu/memory 一致），plan 草稿仅展示 containers 路径。

### 7. 验证策略
- 用户在执行阶段指示「只跑 `cargo check`，不要 build」，故每个 Task 的验证步骤实际改为 `cargo check --release -p libscheduler --tests`（含 `-p common --tests`）。最终又跑了一次 `cargo check --release --workspace --tests` 确认全工作区通过。
- 由于不跑测试，所有计划中的 `cargo test ...` 命令仅作为「测试编译能通过 + 设计完整」的依据，运行验证留待 CI 或人工触发。

### 8. 额外触达的文件（计划未列出）
新数据字段缺省值要求所有既有构造点都更新一次 `..Default::default()`，跨多个 crate：
- `libscheduler/src/plugins/pod_affinity.rs`
- `libscheduler/tests/edge_cases.rs`、`libscheduler/tests/test_scheduler.rs`、`libscheduler/tests/xline_test.rs`
- `rks/tests/test_garbage_collector.rs`、`rks/tests/test_scheduler.rs`
- `rkl/src/daemon/pod_worker.rs`、`rkl/src/daemon/status/probe/probe_manager.rs`、`rkl/src/daemon/status/status_manager.rs`

### 9. 实际提交序列

| 顺序 | Hash | 主题 | 对应 Task |
|---|---|---|---|
| 1 | `9378a73d9` | feat(libscheduler): add GPU/Gang/Topology fields to data model | Task 1 |
| 2 | `300dfbd9d` | feat(libscheduler): add GangStateStore for cross-cycle gang membership | Task 2 |
| 3 | `58619236b` | feat(libscheduler): track GPU requested count in Cache assume/unassume | Task 3 |
| 4 | `f89d9abd8` | feat(libscheduler): add NodeGpuResourcesFit and TopologyCoAffinityFilter plugins | **Task 4 + 5 + 6** |
| 5 | `329857cd1` | feat(libscheduler): defer Assignment send until full gang assumed | Task 7 |
| 6 | `26aeeb5d6` | feat(libscheduler): rollback timed-out gang assumptions | Task 8 |
| 7 | `06d6c029d` | feat(libscheduler): parse GPU/Gang/Topology fields from xline payloads | Task 9 |
| 8 | `75408f43e` | test(libscheduler): add LLM inference E2E scheduling test | Task 10 |
| 9 | `31444abd8` | fix(rkl): supply new common::PodSpec gang/topology_constraints fields in test fixtures | Task 1 收尾 |

总计 9 个提交（计划估算 10 个，实际因 4-5-6 合并为 9 个），全部带 `--no-verify`、不带 AI 身份签名。
