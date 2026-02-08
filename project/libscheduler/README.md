# rk8s 调度器

rk8s 的 Pod 调度模块，提供与 Xline 集成的自动化调度器实现。调度器监听 Xline 中的节点和 Pod 变化，自动进行调度决策。

## 快速开始

### 与 Xline 集成（推荐使用方式）

调度器的主要使用模式是与 Xline 集成，自动监听节点和 Pod 的变化：

```rust
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::with_xline::run_scheduler_with_xline;
use libvault::storage::xline::XlineOptions;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 配置 Xline 连接
    let xline_options = XlineOptions::new(vec!["127.0.0.1:2379".to_string()]);
    
    // 创建取消假定通道（用于处理绑定失败的情况）
    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    
    // 启动调度器并监听 Xline
    let mut rx = run_scheduler_with_xline(
        xline_options,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await?;
    
    // 接收调度结果
    while let Some(result) = rx.recv().await {
        match result {
            Ok(assignment) => {
                println!("Pod {} scheduled to node {}", assignment.pod_name, assignment.node_name);
            }
            Err(e) => {
                eprintln!("Scheduling error: {}", e);
            }
        }
    }
    
    Ok(())
}
```

调度器监听以下 Xline 事件：

#### 节点事件
- **PUT 事件**：节点添加或更新
  - 调度器更新缓存中的节点信息
  - 触发队列提示，重新评估不可调度 Pod
- **DELETE 事件**：节点删除
  - 从缓存中移除节点
  - 将节点上的 Pod 重新加入调度队列

#### Pod 事件
- **PUT 事件**：Pod 添加或更新
  - 如果 Pod 没有 `nodeName`：加入调度队列
  - 如果 Pod 有 `nodeName`：假定已调度，更新缓存
- **DELETE 事件**：Pod 删除
  - 从缓存中移除 Pod
  - 释放节点资源

### 数据存储格式

直接按照 rk8s 的 common crate 中指定的 Pod 和 Node 信息格式存储到 Xline 中即可。

## 架构概述

调度器采用多阶段插件架构，主要包括以下组件：

### 核心组件

1. **调度器 (Scheduler)**：主调度逻辑，管理调度周期和插件执行
2. **调度队列 (SchedulingQueue)**：管理待调度 Pod 的队列，包括：
   - 活动队列 (Active Queue)：按优先级排序的待调度 Pod
   - 回退队列 (Backoff Queue)：调度失败需要等待的 Pod
   - 不可调度队列 (Unschedulable Queue)：暂时无法调度的 Pod
3. **缓存 (Cache)**：存储集群状态（节点和 Pod 信息）
4. **周期状态 (CycleState)**：调度周期内的共享状态存储
5. **插件系统 (Plugins)**：可扩展的插件接口和实现
6. **Xline 监听器**：监听 Xline 变化并更新缓存

### 调度流程

1. **Xline 监听**：监听 `/registry/nodes/` 和 `/registry/pods/` 路径的变化
2. **缓存更新**：根据 Xline 事件更新内部缓存
3. **入队阶段**：Pod 添加到调度队列，执行预入队插件
4. **调度循环**：
   - 预过滤阶段：执行预过滤插件，筛选节点
   - 过滤阶段：执行过滤插件，排除不合适节点
   - 预评分阶段：执行预评分插件，准备评分
   - 评分阶段：执行评分插件，为节点打分
   - 选择节点：选择最高分节点进行绑定
5. **绑定阶段**：预留、许可、预绑定、绑定、后绑定插件执行


## 插件系统

### 插件类型

调度器支持以下类型的插件：

| 插件类型 | 描述 | 默认插件 |
|----------|------|----------|
| `PreEnqueue` | 入队前检查 | `SchedulingGates` |
| `PreFilter` | 调度周期开始，过滤节点 | `NodeAffinity`, `NodeResourcesFit`, `PodAffinity` |
| `Filter` | 节点过滤 | `NodeAffinity`, `NodeResourcesFit`, `TaintToleration`, `NodeName`, `NodeUnschedulable`, `PodAffinity` |
| `PreScore` | 评分前准备 | `NodeAffinity`, `NodeResourcesFit`, `NodeResourcesBalancedAllocation`, `TaintToleration`, `PodAffinity` |
| `Score` | 节点评分 | `NodeAffinity`, `NodeResourcesFit`, `NodeResourcesBalancedAllocation`, `TaintToleration`, `PodAffinity` |
| `EnqueueExtension` | 入队扩展，注册事件提示 | `NodeResourcesBalancedAllocation`, `NodeAffinity`, `NodeName`, `NodeResourcesFit`, `TaintToleration`, `PodAffinity` |

### 内置插件

1. **NodeResourcesFit**：检查节点资源是否满足 Pod 需求
2. **NodeAffinity**：处理节点亲和性规则
3. **PodAffinity**：处理 Pod 亲和性和反亲和性
4. **TaintToleration**：处理污点和容忍度
5. **NodeName**：检查节点名称指定
6. **NodeUnschedulable**：检查节点是否可调度
7. **SchedulingGates**：检查调度门控
8. **NodeResourcesBalancedAllocation**：平衡节点资源分配

### 插件注册

插件通过 `Registry` 结构注册，默认包含所有内置插件：

```rust
impl Default for Registry {
    fn default() -> Self {
        let node_affinity = Arc::new(node_affinity::NodeAffinity {});
        let node_name = Arc::new(NodeName {});
        let fit = Arc::new(Fit {});
        let node_unschedulable = Arc::new(NodeUnschedulable {});
        let scheduling_gates = Arc::new(SchedulingGates {});
        let taint_toleration = Arc::new(TaintToleration {});
        let balanced_allocation = Arc::new(BalancedAllocation::default());
        let pod_affinity = Arc::new(pod_affinity::PodAffinityPlugin);

        Self { /* ... */ }
    }
}
```

## 使用方法

### 基本使用（独立模式）

如果不使用 Xline 集成，也可以直接使用调度器 API：

```rust
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::scheduler::Scheduler;
use libscheduler::models::{NodeInfo, PodInfo, PodSpec, ResourcesRequirements};
use std::collections::HashMap;

// 创建调度器
let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

// 添加节点
let node = NodeInfo {
    name: "node1".to_string(),
    allocatable: ResourcesRequirements { cpu: 4000, memory: 8 * 1024 * 1024 * 1024 },
    requested: ResourcesRequirements { cpu: 0, memory: 0 },
    ..Default::default()
};
scheduler.update_cache_node(node).await;

// 添加 Pod
let pod = PodInfo::new(
    "pod1".to_string(),
    HashMap::new(),
    PodSpec {
        resources: ResourcesRequirements { cpu: 1000, memory: 1024 * 1024 * 1024 },
        priority: 10,
        ..Default::default()
    }
);
scheduler.update_cache_pod(pod).await;

// 启动调度器
let mut rx = scheduler.run();

// 接收调度结果
if let Some(Ok(assignment)) = rx.recv().await {
    println!("Pod {} scheduled to node {}", assignment.pod_name, assignment.node_name);
}
```

### 自定义插件配置

```rust
use libscheduler::plugins::{PluginInfo, Plugins};

// 创建自定义插件配置
let mut plugins = Plugins::default();

// 修改插件权重
plugins.score = vec![
    PluginInfo::with_weight("NodeAffinity", 2),
    PluginInfo::with_weight("NodeResourcesFit", 1),
    PluginInfo::with_weight("NodeResourcesBalancedAllocation", 1),
    PluginInfo::with_weight("TaintToleration", 3),
    PluginInfo::with_weight("PodAffinity", 2),
];

// 使用自定义插件创建调度器
let scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, plugins);
```

## API 参考

### 主要函数

| 函数 | 描述 |
|------|------|
| `run_scheduler_with_xline(xline_options, strategy, plugins, unassume_rx)` | 启动与 Xline 集成的调度器 |

### Scheduler 主要方法

| 方法 | 描述 |
|------|------|
| `new(strategy: ScoringStrategy, plugins: Plugins) -> Self` | 创建调度器实例 |
| `run(&self) -> UnboundedReceiver<Result<Assignment, anyhow::Error>>` | 启动调度器，返回结果通道 |
| `enqueue(&self, pod: PodInfo)` | 将 Pod 加入调度队列 |
| `update_cache_pod(&mut self, pod: PodInfo)` | 更新缓存中的 Pod 信息 |
| `remove_cache_pod(&mut self, pod_name: &str)` | 从缓存移除 Pod |
| `update_cache_node(&mut self, node: NodeInfo)` | 更新缓存中的节点信息 |
| `remove_cache_node(&mut self, node_name: &str)` | 从缓存移除节点 |
| `set_cache_node(&mut self, nodes: Vec<NodeInfo>)` | 批量设置节点 |
| `unassume(&mut self, pod_name: &str)` | 取消 Pod 的假定调度 |

### 评分策略

```rust
#[derive(Clone, Default)]
pub enum ScoringStrategy {
    #[default]
    LeastAllocated,          // 最少分配策略
    MostAllocated,           // 最多分配策略
    RequestedToCapacityRatio, // 请求容量比策略
}
```

## Xline 事件处理



## 配置选项

### 插件权重配置

插件权重在 `PluginInfo` 中配置，仅对评分插件有效：

```rust
PluginInfo::with_weight("NodeAffinity", 2)  // 权重为 2
PluginInfo::new("NodeName")                  // 权重为 0（过滤插件）
```

### 队列配置

调度队列自动管理，支持以下配置：
- 回退时间：指数退避，最大尝试次数 8 次
- 不可调度超时：5 分钟
- 队列刷新间隔：活动队列 1 秒，不可调度队列 30 秒

## 测试

项目包含完整的测试套件，覆盖主要功能：

```bash
# 运行所有测试
cargo test

# 运行 Xline 集成测试
cargo test --test xline_test

# 运行调度器功能测试
cargo test --test test_scheduler
```

### 测试示例

测试文件位于 `tests/` 目录：
- `test_scheduler.rs`：主要功能测试
- `xline_test.rs`：Xline 集成测试
- `edge_cases.rs`：边界情况测试

## 开发指南

### 添加新插件

1. 在 `src/plugins/` 目录创建新插件文件
2. 实现相应的插件 trait
3. 在 `src/plugins/mod.rs` 中注册插件
4. 在 `Registry::default()` 中添加插件实例


### 状态管理

使用 `CycleState` 在插件间共享数据：

```rust
// 写入状态
state.write("MyPluginState", Box::new(MyState { /* ... */ }));

// 读取状态
let state_data = state.read::<MyState>("MyPluginState");
```

# rk8s Scheduler

The Pod scheduling module for rk8s, providing an automated scheduler implementation integrated with Xline. The scheduler listens to node and Pod changes in Xline and performs scheduling decisions automatically.

## Quick Start

### Integration with Xline (Recommended)

The primary usage mode is integrating with Xline to automatically listen for node and Pod changes:

```rust
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::with_xline::run_scheduler_with_xline;
use libvault::storage::xline::XlineOptions;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Configure Xline connection
    let xline_options = XlineOptions::new(vec!["127.0.0.1:2379".to_string()]);
    
    // Create unassume channel (for handling binding failures)
    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    
    // Start scheduler and listen to Xline
    let mut rx = run_scheduler_with_xline(
        xline_options,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await?;
    
    // Receive scheduling results
    while let Some(result) = rx.recv().await {
        match result {
            Ok(assignment) => {
                println!("Pod {} scheduled to node {}", assignment.pod_name, assignment.node_name);
            }
            Err(e) => {
                eprintln!("Scheduling error: {}", e);
            }
        }
    }
    
    Ok(())
}
```

The scheduler listens to the following Xline events:

#### Node Events
- **PUT Event**: Node added or updated
  - Scheduler updates node information in cache
  - Triggers queue hint to re-evaluate unschedulable Pods
- **DELETE Event**: Node deleted
  - Removes node from cache
  - Re-queues Pods on the node to scheduling queue

#### Pod Events
- **PUT Event**: Pod added or updated
  - If Pod has no `nodeName`: added to scheduling queue
  - If Pod has `nodeName`: assumed scheduled, updates cache
- **DELETE Event**: Pod deleted
  - Removes Pod from cache
  - Releases node resources

### Data Storage Format

Simply store Pod and Node information in Xline according to the format specified in the rk8s common crate.

## Architecture Overview

The scheduler adopts a multi-stage plugin architecture, including the following components:

### Core Components

1. **Scheduler**: Main scheduling logic, manages scheduling cycles and plugin execution
2. **SchedulingQueue**: Manages the queue of pending Pods, including:
   - Active Queue: Pending Pods sorted by priority
   - Backoff Queue: Pods that failed scheduling and need to wait
   - Unschedulable Queue: Pods temporarily unable to be scheduled
3. **Cache**: Stores cluster state (node and Pod information)
4. **CycleState**: Shared state storage during scheduling cycles
5. **Plugins**: Extensible plugin interface and implementations
6. **Xline Listener**: Listens to Xline changes and updates cache

### Scheduling Flow

1. **Xline Listening**: Listen to changes in `/registry/nodes/` and `/registry/pods/` paths
2. **Cache Update**: Update internal cache based on Xline events
3. **Enqueue Phase**: Pod added to scheduling queue, pre-enqueue plugins executed
4. **Scheduling Loop**:
   - Pre-filter Phase: Execute pre-filter plugins to filter nodes
   - Filter Phase: Execute filter plugins to exclude unsuitable nodes
   - Pre-score Phase: Execute pre-score plugins to prepare for scoring
   - Score Phase: Execute score plugins to score nodes
   - Select Node: Select highest-scored node for binding
5. **Binding Phase**: Reserve, permit, pre-bind, bind, and post-bind plugin execution

## Plugin System

### Plugin Types

The scheduler supports the following plugin types:

| Plugin Type | Description | Default Plugins |
|-------------|-------------|-----------------|
| `PreEnqueue` | Pre-enqueue checks | `SchedulingGates` |
| `PreFilter` | Start of scheduling cycle, filter nodes | `NodeAffinity`, `NodeResourcesFit`, `PodAffinity` |
| `Filter` | Node filtering | `NodeAffinity`, `NodeResourcesFit`, `TaintToleration`, `NodeName`, `NodeUnschedulable`, `PodAffinity` |
| `PreScore` | Pre-scoring preparation | `NodeAffinity`, `NodeResourcesFit`, `NodeResourcesBalancedAllocation`, `TaintToleration`, `PodAffinity` |
| `Score` | Node scoring | `NodeAffinity`, `NodeResourcesFit`, `NodeResourcesBalancedAllocation`, `TaintToleration`, `PodAffinity` |
| `EnqueueExtension` | Enqueue extension, register event hints | `NodeResourcesBalancedAllocation`, `NodeAffinity`, `NodeName`, `NodeResourcesFit`, `TaintToleration`, `PodAffinity` |

### Built-in Plugins

1. **NodeResourcesFit**: Checks if node resources meet Pod requirements
2. **NodeAffinity**: Handles node affinity rules
3. **PodAffinity**: Handles Pod affinity and anti-affinity
4. **TaintToleration**: Handles taints and tolerations
5. **NodeName**: Checks node name specification
6. **NodeUnschedulable**: Checks if node is schedulable
7. **SchedulingGates**: Checks scheduling gates
8. **NodeResourcesBalancedAllocation**: Balances node resource allocation

### Plugin Registration

Plugins are registered through the `Registry` structure, containing all built-in plugins by default:

```rust
impl Default for Registry {
    fn default() -> Self {
        let node_affinity = Arc::new(node_affinity::NodeAffinity {});
        let node_name = Arc::new(NodeName {});
        let fit = Arc::new(Fit {});
        let node_unschedulable = Arc::new(NodeUnschedulable {});
        let scheduling_gates = Arc::new(SchedulingGates {});
        let taint_toleration = Arc::new(TaintToleration {});
        let balanced_allocation = Arc::new(BalancedAllocation::default());
        let pod_affinity = Arc::new(pod_affinity::PodAffinityPlugin);

        Self { /* ... */ }
    }
}
```

## Usage

### Basic Usage (Standalone Mode)

If not using Xline integration, you can directly use the scheduler API:

```rust
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::scheduler::Scheduler;
use libscheduler::models::{NodeInfo, PodInfo, PodSpec, ResourcesRequirements};
use std::collections::HashMap;

// Create scheduler
let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

// Add node
let node = NodeInfo {
    name: "node1".to_string(),
    allocatable: ResourcesRequirements { cpu: 4000, memory: 8 * 1024 * 1024 * 1024 },
    requested: ResourcesRequirements { cpu: 0, memory: 0 },
    ..Default::default()
};
scheduler.update_cache_node(node).await;

// Add Pod
let pod = PodInfo::new(
    "pod1".to_string(),
    HashMap::new(),
    PodSpec {
        resources: ResourcesRequirements { cpu: 1000, memory: 1024 * 1024 * 1024 },
        priority: 10,
        ..Default::default()
    }
);
scheduler.update_cache_pod(pod).await;

// Start scheduler
let mut rx = scheduler.run();

// Receive scheduling results
if let Some(Ok(assignment)) = rx.recv().await {
    println!("Pod {} scheduled to node {}", assignment.pod_name, assignment.node_name);
}
```

### Custom Plugin Configuration

```rust
use libscheduler::plugins::{PluginInfo, Plugins};

// Create custom plugin configuration
let mut plugins = Plugins::default();

// Modify plugin weights
plugins.score = vec![
    PluginInfo::with_weight("NodeAffinity", 2),
    PluginInfo::with_weight("NodeResourcesFit", 1),
    PluginInfo::with_weight("NodeResourcesBalancedAllocation", 1),
    PluginInfo::with_weight("TaintToleration", 3),
    PluginInfo::with_weight("PodAffinity", 2),
];

// Create scheduler with custom plugins
let scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, plugins);
```

## API Reference

### Main Functions

| Function | Description |
|----------|-------------|
| `run_scheduler_with_xline(xline_options, strategy, plugins, unassume_rx)` | Start scheduler integrated with Xline |

### Main Scheduler Methods

| Method | Description |
|--------|-------------|
| `new(strategy: ScoringStrategy, plugins: Plugins) -> Self` | Create scheduler instance |
| `run(&self) -> UnboundedReceiver<Result<Assignment, anyhow::Error>>` | Start scheduler, return result channel |
| `enqueue(&self, pod: PodInfo)` | Add Pod to scheduling queue |
| `update_cache_pod(&mut self, pod: PodInfo)` | Update Pod information in cache |
| `remove_cache_pod(&mut self, pod_name: &str)` | Remove Pod from cache |
| `update_cache_node(&mut self, node: NodeInfo)` | Update node information in cache |
| `remove_cache_node(&mut self, node_name: &str)` | Remove node from cache |
| `set_cache_node(&mut self, nodes: Vec<NodeInfo>)` | Batch set nodes |
| `unassume(&mut self, pod_name: &str)` | Cancel assumed scheduling for Pod |

### Scoring Strategies

```rust
#[derive(Clone, Default)]
pub enum ScoringStrategy {
    #[default]
    LeastAllocated,          // Least allocated strategy
    MostAllocated,           // Most allocated strategy
    RequestedToCapacityRatio, // Requested to capacity ratio strategy
}
```

## Xline Event Handling

## Configuration Options

### Plugin Weight Configuration

Plugin weights are configured in `PluginInfo`, only effective for scoring plugins:

```rust
PluginInfo::with_weight("NodeAffinity", 2)  // Weight of 2
PluginInfo::new("NodeName")                  // Weight of 0 (filter plugin)
```

### Queue Configuration

Scheduling queue is automatically managed with the following configurations:
- Backoff time: Exponential backoff, maximum 8 attempts
- Unschedulable timeout: 5 minutes
- Queue refresh interval: Active queue 1 second, unschedulable queue 30 seconds

## Testing

The project includes a complete test suite covering main functionality:

```bash
# Run all tests
cargo test

# Run Xline integration tests
cargo test --test xline_test

# Run scheduler functionality tests
cargo test --test test_scheduler
```

### Test Examples

Test files are located in the `tests/` directory:
- `test_scheduler.rs`: Main functionality tests
- `xline_test.rs`: Xline integration tests
- `edge_cases.rs`: Edge case tests

## Development Guide

### Adding New Plugins

1. Create new plugin file in `src/plugins/` directory
2. Implement corresponding plugin trait
3. Register plugin in `src/plugins/mod.rs`
4. Add plugin instance in `Registry::default()`

### State Management

Use `CycleState` to share data between plugins:

```rust
// Write state
state.write("MyPluginState", Box::new(MyState { /* ... */ }));

// Read state
let state_data = state.read::<MyState>("MyPluginState");
```