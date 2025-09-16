# etcd-client

[![Minimum rustc version](https://img.shields.io/badge/rustc-1.80+-lightgray.svg)](https://github.com/etcdv3/etcd-client#rust-version-requirements)
[![Crate](https://img.shields.io/crates/v/etcd-client.svg)](https://crates.io/crates/etcd-client)
[![API](https://docs.rs/etcd-client/badge.svg)](https://docs.rs/etcd-client)

[![License: Apache](https://img.shields.io/badge/License-Apache%202.0-red.svg)](LICENSE-APACHE)
OR
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)

An [etcd](https://github.com/etcd-io/etcd) v3 API client for Rust.
It provides asynchronous client backed by [tokio](https://github.com/tokio-rs/tokio)
and [tonic](https://github.com/hyperium/tonic).

## Features

- etcd API v3
- asynchronous

## Supported APIs

- [x] KV
- [x] Watch
- [x] Lease
- [x] Auth
- [x] Maintenance
- [x] Cluster
- [x] Lock
- [x] Election
- [x] Namespace

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
etcd-client = "0.16"
tokio = { version = "1.0", features = ["full"] }
```

To get started using `etcd-client`:

```rust
use etcd_client::{Client, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut client = Client::connect(["localhost:2379"], None).await?;
    // put kv
    client.put("foo", "bar", None).await?;
    // get kv
    let resp = client.get("foo", None).await?;
    if let Some(kv) = resp.kvs().first() {
        println!("Get kv: {{{}: {}}}", kv.key_str()?, kv.value_str()?);
    }

    Ok(())
}
```

## Examples

Examples can be found in [`examples`](./examples).

## Feature Flags

- `tls`: Enables the `rustls`-based TLS connection. Not enabled by default.
- `tls-roots`: Adds system trust roots to `rustls`-based TLS connection using the `rustls-native-certs` crate. Not
  enabled by default.
- `pub-response-field`: Exposes structs used to create regular `etcd-client` responses including internal protobuf
  representations. Useful for mocking. Not enabled by default.
- `tls-openssl`: Enables the `openssl`-based TLS connections. This would make your binary dynamically link to `libssl`.
- `tls-openssl-vendored`: Like `tls-openssl`, however compile openssl from source code and statically link to it.
- `build-server`: Builds a server variant of the etcd protobuf and re-exports it under the same `proto` package as the `pub-response-field` feature does.
- `raw-channel`: Allows the caller to construct the underlying Tonic channel used by the client.

## Test

We test this library with etcd 3.5.

Note that we use a fixed `etcd` server URI (localhost:2379) to connect to etcd server.

## Rust version requirements

The minimum supported version is 1.80. The current `etcd-client` version is not guaranteed to build on Rust versions
earlier than the minimum supported version.

## License

Dual-licensed to be compatible with the Rust project.

Licensed under the Apache License, Version 2.0 http://www.apache.org/licenses/LICENSE-2.0 or the MIT
license http://opensource.org/licenses/MIT, at your option. This file may not be copied, modified, or distributed except
according to those terms.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `etcd-client` by you, shall be licensed as Apache-2.0 and MIT, without any additional
terms or conditions.
