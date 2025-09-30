---
sidebar_position: 2
title: 安装
---

# 安装 RustyVault

RustyVault 必须在您的环境中正确安装才能正常工作。目前 RustyVault 仅通过源代码提供。RustyVault 可以作为应用程序或库使用，因此：

1. RustyVault 只能从源代编译，或
2. RustyVault 可以在 [crates.io](https://crates.io/crates/rusty_vault) 上获取，以供其他 Rust 项目使用。

本文档是关于如何构建和安装 RustyVault 的应用程序形式。有关库形式，请访问 [docs.rs](https://docs.rs/rusty_vault/latest/rusty_vault) 以获取更多信息。

## 操作系统

RustyVault 可以在以下操作系统上正常工作：

* Linux
* macOS
* Windows (实验性)

在本文档中，macOS 作为演示操作系统。

## 先决条件

RustyVault 是用 [Rust](https://rust-lang.org) 编写的，因此在构建 RustyVault 之前必须在您的环境中正确安装 Rust。

阅读 [这里](https://www.rust-lang.org/tools/install) 以使 Rust 适用于您。

## 从源码构建

从 Github 上拉取最新的 RustyVault 源码：

~~~bash
git clone https://github.com/Tongsuo-Project/RustyVault.git
~~~

然后进入 RustyVault 目录。

~~~bash
cd RustyVault
~~~

使用 Cargo 工具构建二进制文件。

~~~bash
cargo build
~~~

Rust 工具链负责在构建过程中几乎处理所有事情。构建成功后，您将在 `RustyVault/target/debug` 目录中获得一组文件。其中将有一个名为 `rvault` 的可执行文件，这是 RustyVault 的应用程序。

## 验证 RustyVault

运行以下命令：

~~~bash
target/debug/rvault --help
~~~

您将得到类似的响应：

~~~bash
A secure and high performance secret management software that is compatible with Hashicorp Vault.

Usage: rvault [COMMAND]

Commands:
  server  Start a rusty_vault server
  status  Print seal and HA status
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
~~~

这意味着您现在有一个可用的 RustyVault 二进制文件了。
