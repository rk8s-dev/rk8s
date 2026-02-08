# Xline

![Xline-logo](doc/img/xline-horizontal-black.png)

[English](README.md) | 简体中文

本目录为 Xline 在 `rk8s` 单仓（monorepo）中的落地版本，用于二次开发与集成。

- 当前仓库位置：https://github.com/rk8s-dev/rk8s/tree/main/project/Xline
- 上游项目：https://github.com/xline-kv/Xline

[![Discord Shield][discord-badge]][discord-url]
[![Apache 2.0 licensed][apache-badge]][apache-url]
[discord-badge]: https://discordapp.com/api/guilds/1110584535248281760/widget.png?style=shield
[discord-url]: https://discord.gg/hqDRtYkWzm
[apache-badge]: https://img.shields.io/badge/license-Apache--2.0-brightgreen
[apache-url]: https://github.com/rk8s-dev/rk8s/blob/main/project/Xline/LICENSE

欢迎来到 Xline 项目！

`Xline` 致力于在广域网（WAN）的跨数据中心场景中，提供高性能、强一致的元数据管理能力。

## 范围（Scope）

从整体上看，我们期望 Xline 的范围聚焦在如下能力：

- 云原生，兼容 Kubernetes 生态。
- 支持对地理分布式部署更友好。
- 支持丰富的键值（KV）接口，并与 etcd API 完全兼容。
- 在广域网环境中确保高性能与强一致性，底层共识协议采用 CURP。

![cncf-logo](./doc/img/cncf-logo.png)

Xline 是 [Cloud Native Computing Foundation](https://cncf.io/)（CNCF）的 Sandbox 项目。

如果你有任何问题或建议，或希望参与 Xline 的讨论，欢迎加入我们的 [Discord 频道][discord-url]。

## 动机（Motivation）

随着云计算的广泛落地，多云平台（多个公有云或混合云）逐渐成为企业客户的主流 IT 架构。但在一定程度上，多云架构会阻碍不同云之间的数据访问。

同时，云边界带来的数据隔离与数据碎片化也会成为业务增长的阻力。多云架构最大的挑战是：如何在多数据中心的竞争性条件下维持强一致性，并确保高性能。传统的单数据中心方案往往难以满足多数据中心场景对可用性、性能与一致性的要求。

本项目旨在为多云场景提供一种高性能的多云元数据管理方案，这对具有地理分布式与多活部署需求的组织至关重要。

## 创新（Innovation）

跨数据中心网络时延是影响地理分布式系统性能的关键因素之一，尤其是在使用共识协议时。共识协议通常用于实现高可用。例如 etcd 使用 [Raft](https://raft.github.io/) 协议，这是近年来广泛使用的一类共识协议。

尽管 Raft 稳定且易于实现，但从客户端视角看，一次共识请求通常需要 2 次 RTT：一次是客户端到 leader，另一次是 leader 广播到 follower。地理分布式环境中 RTT 往往较大（从几十毫秒到几百毫秒不等），因此 2 RTT 的代价会非常明显。

我们采用一种新的共识协议 [CURP](https://www.usenix.org/system/files/nsdi19-park.pdf) 来解决上述问题。详细描述可参考论文。该协议的主要收益是：在冲突不太高的情况下减少 1 次 RTT。据我们所知，Xline 是第一个采用 CURP 的产品。关于更多协议对比，可参考这篇 [博客](https://zhuanlan.zhihu.com/p/643655556)。

## 性能对比（Performance Comparison）

我们在一个模拟的多集群环境中对比了 Xline 与 etcd，部署细节如下所示。

![test deployment](./doc/img/xline_test_deployment.jpg)

我们使用两种不同 workload 进行对比：一种是单 key 场景，另一种是 100K key 空间场景。测试结果如下。

![xline key perf](./doc/img/xline-key-perf.png)

可以较直观地看到：在地理分布式多集群环境下，Xline 相比 etcd 具备更好的性能表现。

## Xline 客户端

关于 Xline 客户端 SDK 或命令行工具的更多信息，请参阅：

- [Xline client sdk](crates/xline-client/README.md)
- [xlinectl](crates/xlinectl/README.md)

## 快速开始（Quick Start）

请阅读 [QUICK_START.md](doc/QUICK_START.md) 获取更完整的说明与逐步操作指南。

## 贡献指南（Contribute Guide）

我们欢迎社区的任何成员参与贡献。开始贡献请参阅 [CONTRIBUTING.md](./CONTRIBUTING.md)和R2CN.dev。

## 行为准则（Code of Conduct）

Xline 项目遵循 [CNCF 社区行为准则](./CODE_OF_CONDUCT.md)。该准则描述了对贡献者的最低行为期望。

## 路线图（Roadmap）

- v0.1 ~ v0.2
  - 支持所有主要 Etcd API
  - 支持配置文件
  - 通过验证测试（所有已支持的 Etcd API 以及对应的验证测试结果见 [VALIDATION_REPORT.md](./VALIDATION_REPORT.md)）
- v0.3 ~ v0.5
  - 支持持久化存储
  - 支持快照（snapshot）
  - 支持集群成员变更
  - 基本实现一个 k8s operator
- v0.6 ~ v0.8
  - 支持导出 metrics 到监控与告警系统
  - 支持 SSL/TLS 证书
  - 提供多语言客户端（如 Go、Python，尚未确定）。\[注：虽然 Xline 与 etcd 兼容，但我们提供了一个 Xline 专用的客户端 SDK 以获得更好的性能。目前该 SDK 仅支持 Rust，后续计划扩展到更多语言。\]

- v1.0 ~
  - 引入混沌工程以验证系统稳定性
  - 与更多 CNCF 组件集成
  - 支持 Karmada（一个 Kubernetes 管理系统）

## 许可证（License）

参见 [LICENSE](./LICENSE)。
