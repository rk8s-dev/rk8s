# CNI-Plugins 库

该项目提供了在容器和主机之间设置 `veth` 网络接口的功能，并处理网络命名空间（`netns`）。它包括创建和管理网络命名空间、连接接口以及在这些命名空间中配置 veth 设备的支持。未来将补充更多有关CNI插件的功能。

## 文件概述

- **`veth.rs`**：包含 `Veth` 结构体的实现，该结构体表示一对虚拟以太网设备（`veth`）。它包括配置、设置 MAC 地址以及转换为不同表示形式的接口，用于 CNI 响应或网络配置，并实现了（`veth`）的创建。
- **`ns.rs`**：包含 `Netns` 结构体的实现，该结构体表示一个网络命名空间。它包括创建、获取、设置和删除命名空间的方法，以及管理网络命名空间文件描述符的方法。
- **`link.rs`**：实现网络接口连接、设置接口以及管理接口配置的方法。

## 关键概念

### Veth 对

- **veth 对** 由两个网络接口组成，通常用于在主机和容器之间创建连接。这些接口是命名的（例如，容器端为 `veth0`，主机端为 `veth1`），并且这对接口像虚拟网络电缆一样工作，其中在一个接口上发送的数据将在另一个接口上接收。

### 网络命名空间

- **网络命名空间**（`netns`）是一个隔离的网络环境，其中网络接口、路由表和其他网络方面与主机系统和其他容器隔离。每个容器可以被放置在自己的网络命名空间中，以防止容器网络之间的冲突。

---

## 使用方法

### 创建网络命名空间

要创建一个新的网络命名空间：

```rust
use ns::Netns;

let ns = Netns::new().expect("创建网络命名空间失败");
```

### 创建一个命名的网络命名空间

可以创建一个具有特定名称的网络命名空间：

```rust
let ns = Netns::new_named("my_namespace").expect("Failed to create named network namespace");
```

### 删除网络命名空间

要删除之前创建的命名网络命名空间：
```rust
Netns::delete_named("my_namespace").expect("Failed to delete network namespace");
```

### 在指定的网络命名空间中执行命令

要在特定网络命名空间中执行一个函数：
```rust
let result = exec_netns(&current_ns, &target_ns, async {
     // 在 target_ns 中执行操作
    Ok(())
}).await;
```

### 创建一个 Veth 对

要在容器和主机之间创建一个 veth 对：

```rust
let container_ns = Netns::new().expect("Failed to create container namespace");
let host_ns = Netns::get().expect("Failed to get host network namespace");

let container_veth_name = "veth0";
let host_veth_name = "veth1";
let mtu = 1500;
let container_mac = MacAddr::new([0x00, 0x15, 0x5D, 0x1A, 0x1F, 0x1C]);

let veth = setup_veth(
    container_veth_name,
    host_veth_name,
    mtu,
    &container_mac,
    &host_ns,
    &container_ns,
).await.expect("Failed to set up veth pair");
```

---

## 示例流程

- **创建网络命名空间** 为容器分别创建网络命名空间，并获取主机的网络命名空间；
- **创建 veth 对** 为容器和主机创建 veth 对，并设置特定配置（MAC 地址、MTU）；
- **设置命名空间** 为目标容器或主机接口设置命名空间；
- **执行网络操作** 在隔离的命名空间中执行网络操作。

## 错误处理

本项目中的所有函数使用 `anyhow` 库来返回详细的错误信息。如果发生错误，将记录并返回适当的消息以便于调试。

## 依赖项

- `nix`:用于命名空间和挂载管理
- `rtnetlink`:用于与网络设备交互
- `rand`:用于生成随机的 veth 名称
- `log`:用于日志记录

---
# A Library For CNI-Plugins

This project provides functionality to set up `veth` network interfaces between containers and hosts, as well as handling network namespaces (`netns`). It includes support for creating and managing network namespaces, linking interfaces, and configuring veth devices in these namespaces.

## Files Overview

- **`veth.rs`**: Contains the implementation of the `Veth` struct, which represents a pair of virtual Ethernet devices (`veth`). It includes methods to configure, set MAC addresses, and convert to different representations for CNI responses or network configuration.
- **`ns.rs`**: Contains the implementation of the `Netns` struct, which represents a network namespace. It includes methods to create, get, set, and delete namespaces, as well as managing the network namespace file descriptors.
- **`link.rs`**: Implements network interface linking, setting up interfaces, and managing interface configurations.

## Key Concepts

### Veth Pair

A **veth pair** consists of two network interfaces, typically used to create a link between the host and a container. These interfaces are named (e.g., `veth0` for the container side and `veth1` for the host side), and the pair behaves like a virtual network cable, where data sent on one interface will be received on the other.

### Network Namespace

A **network namespace** (`netns`) is an isolated network environment where network interfaces, routing tables, and other networking aspects are isolated from the host system and other containers. Each container can be placed in its own network namespace to prevent conflicts between container networks.

---

## Usage

### Create a Network Namespace

To create a new network namespace:

```rust
use ns::Netns;

let ns = Netns::new().expect("Failed to create network namespace");
```

### Create a Named Network Namespace

You can create a network namespace with a specific name:

```rust
let ns = Netns::new_named("my_namespace").expect("Failed to create named network namespace");
```

### Delete a Network Namespace

To delete a previously created named namespace:
```rust
Netns::delete_named("my_namespace").expect("Failed to delete network namespace");
```

### Execute Commands in a Network Namespace

To execute a function in a specific network namespace:
```rust
let result = exec_netns(&current_ns, &target_ns, async {
    // Execute operations in target_ns
    Ok(())
}).await;
```

### Create a Veth Pair

To create a veth pair between a container and the host:

```rust
let container_ns = Netns::new().expect("Failed to create container namespace");
let host_ns = Netns::get().expect("Failed to get host network namespace");

let container_veth_name = "veth0";
let host_veth_name = "veth1";
let mtu = 1500;
let container_mac = MacAddr::new([0x00, 0x15, 0x5D, 0x1A, 0x1F, 0x1C]);

let veth = setup_veth(
    container_veth_name,
    host_veth_name,
    mtu,
    &container_mac,
    &host_ns,
    &container_ns,
).await.expect("Failed to set up veth pair");
```

---

## Example Flow

- **Create network namespaces** for both the container and host.
- **Create a veth pair** between the container and host with specific configurations (MAC address, MTU).
- **Set the namespace** for the target container or host interface.
- **Perform network operations** within the isolated namespaces.

## Error Handling

All functions in this project use the `anyhow` crate to return detailed error messages. In case of errors, appropriate messages are logged and returned for easy debugging.

## Dependencies

- `nix`:For namespace and mount management.
- `rtnetlink`:For interacting with network devices.
- `rand`:For generating random veth names.
- `log`:For logging.