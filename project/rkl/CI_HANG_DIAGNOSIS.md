# rkl 单机容器旧测试 CI Hang 诊断

本文件用于记录历史上 rkl 单机容器集成测试在 CI 环境中出现 Hang on 的典型原因与定位线索。

当前仓库已将单机场景的容器生命周期自动化验证入口迁移至 rkforge；原 rkl 单机容器集成测试入口文件已不再保留，避免在 CI 环境中因宿主依赖/权限差异导致长时间阻塞。

## 复现入口（测试文件）

- rkl 历史入口：`project/rkl/tests/test_single_container.rs`、`project/rkl/tests/test_pod.rs`（已移除，不再作为 CI 验证入口）
- 当前 CI 入口（rkforge 生命周期）：[test_container_lifecycle.rs](file:///home/ckd/r2cn/rk8s/project/rkforge/tests/test_container_lifecycle.rs)

历史入口会创建/启动容器，并在 `start` / `delete` 阶段触发 CNI `setup/remove`，属于对宿主网络/权限/依赖假设较强的路径。

## 主要 Hang 点（高概率）

### 1) CNI setup/remove 外部插件阻塞

- `ContainerRunner::setup_container_network()` 里调用 `Libcni::setup(...)`  
  位置：[src/commands/container/mod.rs:L412-L430](file:///home/ckd/r2cn/rk8s/project/rkl/src/commands/container/mod.rs#L412-L430)
- `delete_container()`/`remove_container_network()` 里调用 `Libcni::remove(...)`  
  位置：[src/commands/container/mod.rs:L605-L645](file:///home/ckd/r2cn/rk8s/project/rkl/src/commands/container/mod.rs#L605-L645)

在 CI 上常见触发条件：

- `/opt/cni/bin` 的插件缺失或版本不匹配、`/etc/cni/net.d` 配置异常
- 插件内部等待 iptables/bridge/netns 等系统能力，或等待外部资源（导致进程不退出）
- 该调用链没有全局超时控制：插件进程若卡住，测试线程会一直阻塞

### 2) 镜像引用路径触发 OCI 拉取/解包（次高概率）

如果 `image` 被识别为 OCI 引用而非本地 bundle，解包阶段可能卡在 FUSE mount / copy / unmount：

- `sync_handle_oci_image(...)` → `mount_and_copy_bundle(...)`  
  位置：[libruntime/src/utils/mod.rs:L86-L109](file:///home/ckd/r2cn/rk8s/project/libruntime/src/utils/mod.rs#L86-L109)  
  位置：[libruntime/src/bundle.rs:L307-L339](file:///home/ckd/r2cn/rk8s/project/libruntime/src/bundle.rs#L307-L339)

在 CI 上常见触发条件：缺少 FUSE 能力/权限、挂载点异常、底层文件系统 I/O 卡住。

## 结论

旧 rkl 单机容器测试对宿主网络/权限/依赖的假设过强，且关键路径缺乏超时/隔离控制，导致在 CI 中容易以“无输出、长时间阻塞”的方式失败。

后续验证应以 rkforge 的同步模式生命周期测试为准，并将 CI 工作流切换到 rkforge 测试入口：[rkforge-lifecycle.yml](file:///home/ckd/r2cn/rk8s/.github/workflows/rkforge-lifecycle.yml)  
