---
sidebar_position: 1
title: 快速开始
---

# RustyVault 快速开始


在本文档中，我们将演示启动 RustyVault 服务的几个最低但必要的步骤，并使其具有实际功能。

本快速入门文档包括以下示例：

1. 如何从源码构建 RustyVault 二进制文件
2. 如何启动基本的 RustyVault 服务
3. 如何配置 RustyVault 服务
4. 如何使用 RustyVault 服务存储敏感数据（例如“secrets”）

## 从源码构建

如果您想了解更详细的安装信息，请阅读[install.md](./install.md)。

拉取 RustyVault 代码：

~~~bash
git clone https://github.com/Tongsuo-Project/RustyVault.git

cd RustyVault
~~~

使用 Cargo 构建 RustyVault：

~~~bash
cargo build
~~~

如果构建成功，那么您现在在 `RustyVault/target/debug` 目录中有一个名为 `rvault` 的可执行二进制文件。

## 运行服务

RustyVault 作为守护进程运行在系统中，是一个提供一组 RESTful HTTP API 的服务。因此，在服务器在后台运行后，您可以发送 HTTP 请求来要求服务执行工作。

要启动 RustyVault 服务，需要一个配置文件。与 Hashicorp Vault 一样，RustyVault 也可以解析 HCL 配置文件。一个典型的可用 RustyVault 配置文件示例如下：

~~~conf
storage "file" {
    path    = "./data"
}

listener "tcp" {
    address     = "127.0.0.1:8200"
    tls_disable = "true"
    tls_cert_file = "servercert.pem"
    tls_key_file = "serverkey.pem"
    tls_disable_client_certs = false
    tls_require_and_verify_client_cert = false
}

daemon = true
daemon_user = "paul"
daemon_group = "staff"

work_dir = "/Users/paul/work/tmp/rusty_vault/"

api_addr = "http://127.0.0.1:8200"
log_level = "debug"
pid_file = "rusty_vault.pid"
~~~

你需要更改 `daemon_user`、`daemon_group` 和 `work_dir` 为您的实际环境中的值。然后将其复制粘贴到本地文件中，例如，`rvault.hcl` 放在您的机器上的某个地方。


然后启动服务（假设您仍然在 `RustyVault` 目录中）：

~~~bash
target/debug/rvault server --config /path/to/rvault.hcl
~~~

检查进程是否正在运行：

~~~bash
ps -xa | grep rvault

89174 ??         0:00.46 target/debug/rvault server --config /Users/paul/work/tmp/rvault.hcl
89448 ttys006    0:00.00 grep rvault
~~~

应该有一个后台运行的 `rvault` 进程。

现在服务器正在监听 TCP 端口 8200，并准备接收传入的 HTTP 请求。

## 初始化 RustyVault

在完全可用之前，RustyVault 服务需要初始化。例如，在初始化过程中生成一个主密钥，并用于 `seal` 和 `unseal` RustyVault，因此 RustyVault 中的数据可以正确加密。

在本节中，我们使用命令行工具 `curl` 来操作服务，并使用 `jq` 来解析 HTTP 响应中的 JSON 数据。`jq` 不是必需的，但我们强烈建议您在您的机器上安装它。点击[这里](https://jqlang.github.io/jq/download/)了解有关安装 `jq` 的更多信息。

要启动服务，只需运行：

~~~bash
curl --request PUT --data '{"secret_shares": 1, "secret_threshold": 1}' http://127.0.0.1:8200/v1/sys/init | jq
~~~

响应应该类似于：

~~~json
{
  "keys": [
    "7df5ff90cd9417e04cbb9f6db65e0b16ce265d5108fd07e45bdae1a35bf5da6a"
  ],
  "root_token": "bc9e904b-acff-db3d-4cfd-f575cb36428a"
}
~~~

现在我们有一个用于解封 RustyVault 的密钥和一个根令牌。

你可以通过以下命令来检查初始化状态：

~~~bash
curl http://127.0.0.1:8200/v1/sys/init | jq
{
  "initialized": true
}
~~~

## 解封 RustyVault 服务

当 RustyVault 正确初始化后，它处于 *sealed* 状态。这里的 *sealed* 意味着 RustyVault 中的所有内容都是加密的和受保护的，因此没有人可以使用 RustyVault 的任何功能。您需要 *unseal* 它才能使其完全可用。

要解封，将使用上一节生成的密钥：

~~~bash
curl --request PUT --data '{"key": "7df5ff90cd9417e04cbb9f6db65e0b16ce265d5108fd07e45bdae1a35bf5da6a"}' http://127.0.0.1:8200/v1/sys/unseal | jq
~~~

如果一切顺利，那么将返回一个带有 `sealed: false` 的响应：

~~~json
{
  "sealed": false,
  "t": 1,
  "n": 1,
  "progress": 0
}
~~~

这表明 RustyVault 服务已解封，可以执行更多实际的工作。

## 将 Secrets 写入 RustyVault

RustyVault 的一个常用功能是 *secret*，它上是一个安全的键值存储，可以保留任意敏感值，例如密码、凭证、令牌、密钥等。

RustyVault 需要客户端认证来进行后续操作。在本演示中，我们使用上一节生成的 `root_token` 来简化操作。

让我们请求 RustyVault 将 `foo: bar` 值存储在 `test` 键下：

~~~bash
curl --Header "Cookie: token=bc9e904b-acff-db3d-4cfd-f575cb36428a" --request POST --data '{ "foo": "bar" }' http://127.0.0.1:8200/v1/secret/test | jq
~~~

然后读取它：

~~~bash
curl --Header "Cookie: token=bc9e904b-acff-db3d-4cfd-f575cb36428a" http://127.0.0.1:8200/v1/secret/test | jq
{
  "renewable": false,
  "lease_id": "",
  "lease_duration": 3600,
  "auth": null,
  "data": {
    "foo": "bar"
  }
}
~~~

在响应的 JSON 数据中的 `data` 部分，您可以再次看到 `foo:bar` 值！

## 后续步骤

在本文档中，我们构建了一个 RustyVault 服务，启动了它，并配置它以接受用户命令，例如存储敏感数据。这里的所有示例仅用于演示目的，它们在实际生产场景中可能不安全。RustyVault 中引入了一些更多的功能，使其可以投入生产：

* 身份验证方法：RustyVault 提供了不同的身份验证方法，允许您使用细粒度的访问策略创建新的客户端令牌。
* 更多存储类型：本演示使用本地文件作为存储，但实际上既不高效也不安全。RustyVault 还支持其他远程存储类型，如数据库、远程文件系统等。
* 运行状态：RustyVault 的工作目录中有一个日志文件，其中记录了重要信息，用于调试或其他目的。
* 与 Hashicorp Vault 兼容：RustyVault 与 Hashicorp Vault 兼容，因此大多数 Hashicorp Vault 文档也值得阅读，以帮助您了解 RustyVault ;-)