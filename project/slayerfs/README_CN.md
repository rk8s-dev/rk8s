<div align="center">
	<img src="doc/icon.png" alt="SlayerFS icon" width="96" height="96" />
</div>

<h1 align="center">SlayerFS</h1>
<p align="center"><strong>高性能 Rust &amp; 层感知分布式文件系统</strong></p>
<p align="center"><a href="README.md">English</a> | <a href="README_CN.md"><b>中文</b></a></p>

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)


## ✨ 项目概览

SlayerFS 是一个使用 Rust 构建、面向容器与 AI 场景的分布式文件系统原型（MVP）。它采用 chunk/block 的数据布局，并与对象存储后端对接（LocalFS 已实现；S3/Rustfs 预留），提供基于路径的读写、目录操作、截断等基础能力，便于与 SDK 与 FUSE 集成。

核心理念：计算与存储解耦。应用通过 POSIX 风格接口访问数据，由调度/缓存层决定数据的驻留位置与访问路径。

## 🖼 架构

<div align="center">
	<img src="doc/SlayerFS.png" alt="SlayerFS architecture" width="720" />
</div>

组件概览：
- chuck：ChunkLayout、ChunkReader/Writer，负责将文件偏移映射到 chunk/block，处理跨块 IO 与洞零填充；
- cadapter：对象后端抽象与实现（LocalFs 已实现，S3/Rustfs 预留）；
- meta：内存版元数据与事务（InMemoryMetaStore），记录 size 与 slice，支持提交/回滚；
- fs：基于路径的 FileSystem（mkdir/mkdir_all/create/read/write/readdir/stat/unlink/rmdir/rename/truncate）；
- vfs：面向 FUSE 的 inode-based VFS；
- sdk：面向应用的轻量客户端封装（基于 FileSystem，提供 LocalClient 便捷构造）。

## 🚀 快速开始

### 环境要求

- Rust: >= 1.75.0
- 操作系统：Linux (Ubuntu 20.04+, CentOS 8+)

```bash
cargo run -q --bin sdk_demo -- /tmp/slayerfs-objroot
```
示例将会：
- 创建多级目录/文件，进行跨 block/chunk 写入与读回校验；
- 执行重命名、截断（收缩/扩展）、列目录与删除；
- 打印预期错误场景，并输出 "sdk demo: OK"。

---

## 🌟 当前能力（MVP）

### 基于路径的 FileSystem
- mkdir/mkdir_all/create/read/write/readdir/stat/exists/unlink/rmdir/rename/truncate
- 使用单把互斥锁保护命名空间（避免多锁死锁）；热点路径避免持锁 await

### 分块 IO + 洞零填充
- 默认 64MiB chunk + 4MiB block（可配置）
- 写路径按 block 拆分；读路径对未写区域返回 0

### 对象存储 BlockStore
- LocalFs 已实现（用于测试/示例）；S3/Rustfs 预留接口

### 带事务的元数据
- InMemoryMetaStore：alloc_inode、record_slice、update_size（支持截断收缩）
- 已覆盖提交/回滚测试

更多细节：参见 `doc/sdk.md` 与源码注释。

---

## 📚 文档
- 设计：`doc/arch.md`
- SDK 使用：`doc/sdk.md`

---

## 🤝 参与贡献

欢迎通过 Issue/PR 参与改进架构、实现与文档。
