# CNI 插件桥接网络配置

本项目提供了一个CNI插件，用于在容器化环境中配置和管理桥接网络接口。包含VLAN管理、错误处理功能，并支持桥接的自定义网络配置。

## 功能特性

- **错误处理**: 使用自定义错误类型 (`AppError` and `VlanError`) 进行详细结构化错误报告，可转换为CNI错误响应；
- **桥接网络配置**: `BridgeNetConf` 结构体提供全面的桥接网络配置模型，支持VLAN、MAC地址过滤等多种网络接口选项；
- **VLAN汇聚**: 支持定义VLAN汇聚范围(`VlanTrunk`)和桥接配置中的VLAN过滤；
- **序列化**: 使用Serde进行配置的序列化和反序列化，确保网络配置文件处理的灵活性；
- **桥接管理**: `Bridge` 结构体抽象了网络桥接，包含设置MTU大小和启用VLAN过滤等方法。

## 核心组件

### 错误处理

定义了两类主要错误类型：

- **AppError**: 涵盖通用错误，包括CNI、VLAN、网络命名空间等相关问题；
- **VlanError**: 专门处理VLAN配置错误，包括无效的trunk ID、缺失参数和ID范围错误；

两种错误类型均可转换为CNI兼容的错误响应。


### 桥接网络配置

 `BridgeNetConf` 结构体扩展自 `NetworkConfig` 包含以下配置选项：

- 桥接名称和网关设置；
- MAC地址欺骗和DAD(重复地址检测)；
- VLAN配置，包括trunk和过滤。

同时支持MTU大小、端口隔离等附加配置。

### 桥接结构体

`Bridge` 结构体表示虚拟网络桥接，包含设置MTU大小和启用VLAN过滤的方法。该结构体可转换为 `LinkMessageBuilder` 用于网络设置。

### 配置示例 

- **配置文件**: my-bridge.conf
```json
{
  "cniVersion": "1.0.0",
  "name": "my-bridge",
  "type": "libbridge",
  "bridge": "br0",
  "isGateway": true,
  "mtu": 1500,
  "vlan": 100,
}
```
- **命令行操作**
```shell
#set up network namespace
sudo ip netns add test

# add bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool add my-bridge /var/run/netns/test

# del bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool del my-bridge /var/run/netns/test
```

### 主要依赖项

- **CNI-plugin**: 处理CNI特定错误和响应的主插件；
- **Serde**: 用于配置数据的序列化和反序列化
- **Rtnetlink**: 用于与Linux网络接口交互(如设置桥接和VLAN)


---
# CNI Plugin Bridge Network Configuration

This project provides a CNI plugin for configuring and managing bridge network interfaces in containerized environments. It includes functionality for managing VLANs, handling errors, and supporting custom network configurations for bridges.

## Features

- **Error Handling**: Custom error types (`AppError` and `VlanError`) are used for detailed and structured error reporting, which can be translated into CNI error replies.
- **Bridge Network Configuration**: The `BridgeNetConf` struct provides an extensive configuration model for setting up bridge networks, including options for VLAN support, MAC address filtering, and various network interface options.
- **VLAN Trunking**: Support for defining VLAN trunking ranges (`VlanTrunk`) and VLAN filtering in bridge configurations.
- **Serialization**: The project uses Serde to serialize and deserialize configurations, ensuring flexibility in handling network configuration files.
- **Bridge Management**: The `Bridge` struct offers an abstraction for a network bridge, including methods for setting MTU size and enabling VLAN filtering.

## Components

### Error Handling

The project defines two main error types:

- **AppError**: Covers general errors, including CNI, VLAN, network namespace, and other related issues.
- **VlanError**: Specifically handles errors related to VLAN configuration, including invalid trunk IDs, missing parameters, and incorrect ID ranges.

Both error types can be converted into CNI-compatible error replies.

### Bridge Network Configuration

The `BridgeNetConf` struct extends `NetworkConfig` and contains options for configuring a bridge network, such as:

- Bridge name and gateway settings.
- MAC address spoofing and DAD (Duplicate Address Detection).
- VLAN configuration, including trunking and filtering.

Additional configurations like MTU size and port isolation are also supported.

### Bridge Struct

The `Bridge` struct is a representation of a virtual network bridge, with methods to set the MTU size and enable VLAN filtering. This struct can be converted into a `LinkMessageBuilder` for further use in network setup.

### Example 

- **Configuration**: my-bridge.conf
```json
{
  "cniVersion": "1.0.0",
  "name": "my-bridge",
  "type": "libbridge",
  "bridge": "br0",
  "isGateway": true,
  "mtu": 1500,
  "vlan": 100,
}
```
- **Shell**
```shell
#set up network namespace
sudo ip netns add test

# add bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool add my-bridge /var/run/netns/test

# del bridge
> sudo \
  env CNI_PATH=/opt/cni/bin:$PWD/target/debug \
  cnitool del my-bridge /var/run/netns/test
```

### Dependencies

- **CNI Plugin**: The main plugin for handling CNI-specific errors and replies.
- **Serde**: For serializing and deserializing configuration data.
- **Rtnetlink**: For interacting with Linux network interfaces (e.g., setting up bridges and VLANs).
