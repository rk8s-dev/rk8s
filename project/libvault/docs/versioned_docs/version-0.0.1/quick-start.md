---
sidebar_position: 1
title: Quick Start
---

# RustyVault Quick Start

In this document, we demonstrate several minimum but necessary steps for starting up a RustyVault server and make it functional for real features.

This quick start document includes examples to:

1. how to build RustyVault binary from source
2. how to start a basic RustyVault server
3. how to configure a RustyVault server
4. how to use a RustyVault server to store sensitive data (the 'secrets', for instance)

## Build from Source

Read [install.md](./install.md) if you want more detailed information on installation.

Clone RustyVault from Github:

~~~bash
git clone https://github.com/Tongsuo-Project/RustyVault.git

cd RustyVault
~~~

Build RustyVault by using Rust toolchain:

~~~bash
cargo build
~~~

If the build is successful, then you now have an executable binary file called `rvault` in `RustyVault/target/debug` directory.

## Run the Server

RustyVault runs as a daemon in the operation system. It's basically a server that provides a set of RESTful HTTP APIs. So after the server is running in the background, you can send HTTP requests to ask the server to do the jobs.

To launch a RustyVault server, a configuration file is needed. As Hashicorp Vault, RustyVault can also parse HCL configuration files. A typical usable example RustyVault configuration file is as follows:

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

You need to change the variables like `daemon_user`, `daemon_group` and `work_dir` to the actual value in your environment. Then just copy and paste it to a local file, say, `rvault.hcl` somewhere on your machine.

Then launch the server (assume you are still in `RustyVault` directory):

~~~bash
target/debug/rvault server --config /path/to/rvault.hcl
~~~

Check the process is running:

~~~bash
ps -xa | grep rvault

89174 ??         0:00.46 target/debug/rvault server --config /Users/paul/work/tmp/rvault.hcl
89448 ttys006    0:00.00 grep rvault
~~~

There should be an `rvault` process running in background.

Now the server is listening on TCP port 8200 and it's ready for incoming HTTP requests.

## Initialize RustyVault

Before it's fully usable, a RustyVault server needs to be initialized. For instance, a master key is generated during the initialization procedure and is used to `seal` and `unseal` RustyVault, thus the data in RustyVault can be correctly encrypted.

In this section, we use command line tool `curl` to manipulate the server and use `jq` to parse the JSON data in the HTTP responses. `jq` is not required, but we highly recommend to install it on your machine. Click [here](https://jqlang.github.io/jq/download/) for more information on installing `jq`.

To launch the server, simply run:

~~~bash
curl --request PUT --data '{"secret_shares": 1, "secret_threshold": 1}' http://127.0.0.1:8200/v1/sys/init | jq
~~~

The response should be similar to this:

~~~json
{
  "keys": [
    "7df5ff90cd9417e04cbb9f6db65e0b16ce265d5108fd07e45bdae1a35bf5da6a"
  ],
  "root_token": "bc9e904b-acff-db3d-4cfd-f575cb36428a"
}
~~~

Now we have a key to unseal RustyVault and a root token.

You can check the initialization status by sending:

~~~bash
curl http://127.0.0.1:8200/v1/sys/init | jq
{
  "initialized": true
}
~~~

## Unseal the RustyVault Server

When RustyVault is initialized properly, it's in the *sealed* status. *Seald* here means everything in RustyVault is encrypted and protected, thus no one can use any functionality RustyVault. You need to *unseal* it to make it fully functional.

To unseal, the key generated in previous section will be used:

~~~bash
curl --request PUT --data '{"key": "7df5ff90cd9417e04cbb9f6db65e0b16ce265d5108fd07e45bdae1a35bf5da6a"}' http://127.0.0.1:8200/v1/sys/unseal | jq
~~~

If everything went smoothly, then a response with `sealed: false` will be returned:

~~~json
{
  "sealed": false,
  "t": 1,
  "n": 1,
  "progress": 0
}
~~~

This indicates the RustyVault server is not sealed and it's ready to do more real jobs.

## Write Secrets to RustyVault

A frequently used feature of RustyVautl is *secret*, it's basically a secure key-value storage that can retain arbitary sensitive values such as password, credentials, tokens, keys and so forth.

RustyVault needs client authentication for further operations. In this demonstration, we utilize the `root_token` generated in previous section for simplicity.

Let's ask RustyVault to store a `foo: bar` value under the key `test`:

~~~bash
curl --Header "Cookie: token=bc9e904b-acff-db3d-4cfd-f575cb36428a" --request POST --data '{ "foo": "bar" }' http://127.0.0.1:8200/v1/secret/test | jq
~~~

Then read it out:

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

In the `data` section of the responsed JSON, you can see the `foo:bar` value once again!

## Next Steps

In this document, we built a RustyVault server, started it up and configured it to accept user commands such as storing sensitive data. All examples here are only for demonstration purposes, they may not safe in real production scenarios. Some more features are introduced in RustyVault to make it production ready:

* Authentication methods: RustyVault offers different authentication methods, which allow you create new client tokens with fine-grained access policy,
* More storage types: This demonstration uses local file as storage, but in reality it's neither efficient nor secure. RustyVault also supports other remote storage types like database, remote file system or so.
* Running status: a log file is located in the working directory of RustyVault, important information is logged in it for debug or other purposes.
* Compatibility with Hashicorp Vault: RustyVault is compatible with Hashicorp Vault, so most Hashicorp Vault documentation is also worth to read to help you understand RustyVault ;-) 