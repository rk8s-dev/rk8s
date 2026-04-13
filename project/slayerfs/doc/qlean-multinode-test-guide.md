# SlayerFS Qlean 多节点集成测试指南

本文档说明如何在本地（包括 **WSL2**）运行 `test_slayerfs_qlean_multinode_smoke` 集成测试，以及所有必要的一次性环境配置步骤。

测试框架：[qlean 0.2.x](https://crates.io/crates/qlean)，通过 QEMU/KVM 启动两个 Debian 13 虚拟机，对三个元数据后端（etcd、redis、postgres）分别运行 fio 负载，验证 SlayerFS 分布式挂载的正确性与基本性能。

---

## 目录

1. [测试架构概览](#1-测试架构概览)
2. [前置依赖](#2-前置依赖)
3. [WSL2 专项配置（首次必做）](#3-wsl2-专项配置首次必做)
4. [通用一次性配置](#4-通用一次性配置)
5. [运行测试](#5-运行测试)
6. [测试结果](#6-测试结果)
7. [常见问题排查](#7-常见问题排查)
8. [重启后的注意事项](#8-重启后的注意事项)

---

## 1. 测试架构概览

```
主机 (Host)
├── docker compose  →  etcd :2379 / redis :6379 / postgres :15432
├── qlean 测试进程  →  通过 vsock SSH 管理 VM
└── libvirt / qlbr0 (192.168.221.1/24)
     ├── client1 VM  (CID 10, IP 192.168.221.x)
     └── client2 VM  (CID 11, IP 192.168.221.y)
          ↑ 两台 VM 挂载 SlayerFS，通过 qlbr0 访问主机上的元数据后端
```

测试流程（`run-distributed-tests.sh all`）：

1. **prepare** — 检查 SSH 连通性和 sudo 权限
2. **deploy-slayerfs** — 上传 `persistence_demo` binary 并挂载 SlayerFS
3. **test-slayerfs** — 在挂载点上运行 fio 基准测试
4. **collect** — 将结果拉回主机 `tests/scripts/distributed-tests/results/`

三个后端（etcd → redis → postgres）顺序执行，各自独立一轮完整流程。

---

## 2. 前置依赖

### 2.1 系统软件包

```bash
sudo apt-get update
sudo apt-get install -y \
    fuse3 \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    qemu-system-x86 \
    qemu-utils \
    libvirt-clients \
    libvirt-daemon-system \
    guestfish \
    linux-image-virtual \
    xorriso
```

> **注意**：`libvirt-daemon-system` 和 `linux-image-virtual` 是 `run_integration_tests.sh`
> 中原有列表之外的额外依赖，WSL2 环境必须安装。

### 2.2 Docker（后端服务）

参考 [docker compose-test-guide.md](docker compose-test-guide.md) 安装 Docker 和 docker compose。

### 2.3 Rust 工具链

```bash
rustup toolchain install stable  # 测试本身用 stable
```

---

## 3. WSL2 专项配置（首次必做）

WSL2 内核不带标准 `/boot` 目录，QEMU 辅助工具（qemu-bridge-helper）也默认缺少 setuid 位。以下步骤**每台机器只需执行一次**。

### 步骤 3.1 — 修复 `/boot` 内核文件权限

`guestfish` 的 `supermin` 组件需要读取宿主机内核镜像来构建 appliance：

```bash
# 确认内核镜像已安装（linux-image-virtual 会写入此路径）
ls /boot/vmlinuz-*

# 将内核文件设为所有用户可读
sudo chmod o+r /boot/vmlinuz-*
```

### 步骤 3.2 — 加载 vhost_vsock 内核模块

qlean 使用 virtio-socket（vsock）而非 TCP 与 VM 建立 SSH 连接，需要此模块：

```bash
sudo modprobe vhost_vsock

# 验证
lsmod | grep vhost_vsock
# 应输出: vhost_vsock  24576  0
```

**使模块开机自动加载**（否则每次重启 WSL2 后需重新执行）：

```bash
echo 'vhost_vsock' | sudo tee /etc/modules-load.d/vhost_vsock.conf
```

### 步骤 3.3 — 启动 libvirtd 并将用户加入相关组

```bash
sudo systemctl start libvirtd
sudo systemctl enable libvirtd   # 可选，WSL2 不完全支持自动启动

sudo usermod -aG kvm    "$USER"
sudo usermod -aG libvirt "$USER"

# 使组变更生效（或重新登录）
newgrp libvirt
newgrp kvm
```

### 步骤 3.4 — 创建 QEMU 网桥配置

qlean 使用 `qlbr0` 网桥，需要告知 QEMU 允许此网桥：

```bash
sudo mkdir -p /etc/qemu
cat <<'EOF' | sudo tee /etc/qemu/bridge.conf
allow virbr0
allow qlbr0
EOF
```

### 步骤 3.5 — 为 qemu-bridge-helper 添加 setuid

`qemu-bridge-helper` 需要 root 权限来操作网桥，必须设置 setuid 位：

```bash
sudo chmod u+s /usr/lib/qemu/qemu-bridge-helper

# 验证（应看到 -rwsr-xr-x）
ls -la /usr/lib/qemu/qemu-bridge-helper
```

### 步骤 3.6 — 验证配置完整性

```bash
# 检查模块
lsmod | grep vhost_vsock

# 检查组成员
groups | grep -E 'kvm|libvirt'

# 检查 libvirtd
systemctl is-active libvirtd

# 检查 bridge.conf
cat /etc/qemu/bridge.conf

# 检查 setuid
ls -la /usr/lib/qemu/qemu-bridge-helper | grep rws
```

---

## 4. 通用一次性配置

以下步骤在所有平台（WSL2 / 原生 Linux）上均需执行一次。

### 步骤 4.1 — 确认内核文件可读（非 WSL2 也建议检查）

```bash
ls -la /boot/vmlinuz-* | head -3
# 若为 ------- 则执行：sudo chmod o+r /boot/vmlinuz-*
```

### 步骤 4.2 — 启动后端服务

在 `project/slayerfs/` 目录下：

```bash
docker compose -f docker/docker compose.integration.yml up -d

# 验证三个服务均健康
docker compose -f docker/docker compose.integration.yml ps
```

### 步骤 4.3 — 构建 persistence_demo

在 `project/` 目录下（即 `slayerfs/` 的父目录）：

```bash
cd /path/to/rk8s/project
cargo build -p slayerfs --example persistence_demo --release
```

> 首次编译约需 5～15 分钟。产物位于 `target/release/examples/persistence_demo`（约 500MB+）。

---

## 5. 运行测试

### 5.1 使用 run_integration_tests.sh（推荐）

脚本已封装完整流程（启动服务 → 构建 → 测试 → 清理）：

```bash
cd /path/to/rk8s/project/slayerfs

# 完整运行（首次需要 --skip-services=false，确保 docker compose 服务已起）
# 注意：必须在 kvm 和 libvirt 组下执行
sg libvirt -c "sg kvm -c './scripts/run_integration_tests.sh --skip-deps'"
```

`--skip-deps` 可在已完成步骤 2.1 后跳过 apt 安装加速。

### 5.2 直接运行 cargo test

在 `project/` 目录下：

```bash
cd /path/to/rk8s/project
sg libvirt -c "sg kvm -c 'cargo test -p slayerfs --test test_slayerfs_qlean_multinode_smoke -- --ignored'"
```

> `sg libvirt -c "sg kvm -c '...'"` 是必须的包装，用于在 kvm 和 libvirt 两个组权限下运行。
> 若已重新登录（`newgrp` 后重登），组已生效则可省略 `sg` 包装。

### 5.3 预期运行时间

| 阶段 | 首次运行 | 后续运行 |
|------|----------|----------|
| VM 镜像下载 | 5～10 分钟 | 已缓存，跳过 |
| VM 启动 + 初始化 | 3～5 分钟 | 3～5 分钟 |
| apt 安装（VM 内） | 2～5 分钟 | 已缓存，跳过 |
| 三个后端各一轮 fio | ~3 分钟 | ~3 分钟 |
| **总计** | **15～25 分钟** | **~9 分钟** |

> VM 镜像缓存位于 `~/.local/share/qlean/images/`。VM overlay 在每次测试后会被清理（`config.clear: true`），但 apt 包在首次 VM 启动时通过 cloud-init 种子安装后会保留在 overlay 中直到测试结束。

---

## 6. 测试结果

### 6.1 结果目录

每次成功运行后，fio 结果会被下载到：

```
project/slayerfs/tests/scripts/distributed-tests/results/
└── slayerfs-results/
    ├── 20260408-081445/   ← etcd 后端
    │   └── slayerfs/fio/{seqwrite,seqread,randwrite,randread}.json
    ├── 20260408-081733/   ← redis 后端
    │   └── slayerfs/fio/...
    └── 20260408-081914/   ← postgres 后端
        └── slayerfs/fio/...
```

目录名格式：`YYYYMMDD-HHMMSS`，每个后端一个独立目录。

### 6.2 成功标志

```
test test_slayerfs_qlean_multinode_smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; finished in 519.17s
```

---

## 7. 常见问题排查

### 问题 1：`supermin: failed to find a suitable kernel`

**原因**：WSL2 的 `/boot` 为空，guestfish 找不到内核。

**解决**：
```bash
sudo apt-get install -y linux-image-virtual
sudo chmod o+r /boot/vmlinuz-*
```

---

### 问题 2：`Permission denied` 读取 `/boot/vmlinuz-*`

**原因**：内核文件权限为 `600`，普通用户无法读取。

**解决**：
```bash
sudo chmod o+r /boot/vmlinuz-*
```

---

### 问题 3：vsock 连接失败 `network unreachable (os error 101)`

**原因**：`vhost_vsock` 内核模块未加载。

**解决**：
```bash
sudo modprobe vhost_vsock
# 永久生效：
echo 'vhost_vsock' | sudo tee /etc/modules-load.d/vhost_vsock.conf
```

---

### 问题 4：`failed to parse default acl file /etc/qemu/bridge.conf`

**原因**：`/etc/qemu/bridge.conf` 不存在，QEMU 拒绝使用 `qlbr0` 网桥。

**解决**：
```bash
sudo mkdir -p /etc/qemu
printf 'allow virbr0\nallow qlbr0\n' | sudo tee /etc/qemu/bridge.conf
```

---

### 问题 5：`failed to get master ioctl: Operation not permitted`（qemu-bridge-helper）

**原因**：`/usr/lib/qemu/qemu-bridge-helper` 缺少 setuid 位。

**解决**：
```bash
sudo chmod u+s /usr/lib/qemu/qemu-bridge-helper
```

---

### 问题 6：`connection refused` 到 `192.168.221.1:2379/6379/15432`

**原因**：docker compose 后端服务未启动，或监听在 `127.0.0.1` 而非 `0.0.0.0`。

**解决**：
```bash
cd project/slayerfs
docker compose -f docker/docker compose.integration.yml up -d
# 验证连通性（从主机）
nc -zv 192.168.221.1 2379 && nc -zv 192.168.221.1 6379 && nc -zv 192.168.221.1 15432
```

> `192.168.221.1` 是 `qlbr0` 网桥在主机侧的 IP（VM 的默认网关），VM 通过此 IP 访问主机上的服务。

---

### 问题 7：`command timed out after 1800s`

**原因**：首次运行时 VM 内需要通过 apt 安装 fio 等工具，耗时较长；或网络抖动导致 apt 下载缓慢。

**解决**：重新运行即可。第二次运行时 VM overlay 中工具已就位，通常可在 9 分钟内完成。

若重复超时，可检查 VM 内的网络：
```bash
# 查看 qlean 是否给 VM 分配了 IP
virsh net-dhcp-leases qlean
```

---

### 问题 8：`Permission denied` 运行 QEMU / virsh

**原因**：当前 shell 的有效组中缺少 `kvm` 或 `libvirt`。

**解决**：
```bash
# 临时生效（每次新终端需执行）
newgrp kvm
newgrp libvirt
# 或使用 sg 包装运行命令（推荐，无需重登）
sg libvirt -c "sg kvm -c 'cargo test ...'"
```

---

## 8. 重启后的注意事项

WSL2 重启后，以下状态会丢失，需要重新配置：

| 状态 | 是否持久 | 重启后操作 |
|------|----------|------------|
| `vhost_vsock` 模块 | ❌ 丢失 | `sudo modprobe vhost_vsock`（或配置了 `/etc/modules-load.d/` 则自动） |
| `libvirtd` 服务 | ❌ 丢失 | `sudo systemctl start libvirtd` |
| docker compose 服务 | ❌ 丢失 | `docker compose -f docker/docker compose.integration.yml up -d` |
| KVM/libvirt 组成员 | ✅ 持久 | 无需操作 |
| `/etc/qemu/bridge.conf` | ✅ 持久 | 无需操作 |
| qemu-bridge-helper setuid | ✅ 持久 | 无需操作 |
| `/boot/vmlinuz-*` 权限 | ✅ 持久 | 无需操作 |
| qlbr0 网桥 | ❌ 丢失 | libvirtd 启动时自动重建 |
| VM 镜像缓存 | ✅ 持久 | 无需操作 |

**WSL2 重启后的快速恢复命令**：

```bash
sudo modprobe vhost_vsock
sudo systemctl start libvirtd
cd /path/to/rk8s/project/slayerfs
docker compose -f docker/docker compose.integration.yml up -d
```
