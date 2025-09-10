# macaddr

> MAC address types for Rust

[![Latest Version](https://img.shields.io/crates/v/macaddr.svg)](https://crates.io/crates/macaddr)
[![Latest Version](https://docs.rs/macaddr/badge.svg)](https://docs.rs/macaddr)
[![Build Status](https://github.com/svartalf/rust-macaddr/workflows/Continuous%20integration/badge.svg)](https://github.com/svartalf/rust-macaddr/actions)
[![Coverage Status](https://coveralls.io/repos/github/svartalf/rust-macaddr/badge.svg?branch=master)](https://coveralls.io/github/svartalf/rust-macaddr?branch=master)
![Minimum rustc version](https://img.shields.io/badge/rustc-1.31+-green.svg)
![Apache 2.0 OR MIT licensed](https://img.shields.io/badge/license-Apache2.0%2FMIT-blue.svg)
![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)

This crate provides types for a [MAC address](https://en.wikipedia.org/wiki/MAC_address)
identifiers, both in IEEE *EUI-48* and *EUI-64* formats.

It is like a [`std::net::SocketAddr`](https://doc.rust-lang.org/std/net/enum.SocketAddr.html) enum with a
[`std::net::SocketAddrV4`](https://doc.rust-lang.org/std/net/struct.SocketAddrV4.html) and
[`std::net::SocketAddrV6`](https://doc.rust-lang.org/std/net/struct.SocketAddrV6.html) members,
but for MAC addresses instead.

Obviously, MAC address can be represented as a `[u8; 6]` or `[u8; 8]`,
but it is error-prone and inconvenient, so here they are —
[MacAddr6](https://docs.rs/macaddr/latest/macaddr/struct.MacAddr6.html) and
[MacAddr8](https://docs.rs/macaddr/latest/macaddr/struct.MacAddr8.html)
structs with helpful methods and standard Rust traits implementations,
intended to be the first-class Rust objects.

And it is `serde`- and `no_std`-friendly also!

## Installation

Add this to your `Cargo.toml`

```toml
[dependencies]
macaddr = "1.0"
```

## Usage

Check out the [documentation](https://docs.rs/macaddr) for each type
available, all of them have a plenty of examples.

## License

Licensed under either of [Apache License 2.0](https://github.com/svartalf/rust-macaddr/blob/master/LICENSE-APACHE)
or [MIT license](https://github.com/svartalf/rust-macaddr/blob/master/LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

