//! This crate provides types for a [MAC address] identifiers,
//! both in IEEE *EUI-48* and *EUI-64* formats.
//!
//! It is like a `std::net::SocketAddr` enum
//! with `std::net::SocketAddrV4` and `std::net::SocketAddrV6` members,
//! but for MAC addresses instead.
//!
//! Obviously, MAC address can be represented as a `[u8; 6]` or `[u8; 8]`,
//! but it is error-prone and inconvenient, so here they are —
//! [MacAddr6] and [MacAddr8] structs with helpful methods and
//! standard Rust traits implementations to make them first-class
//! Rust objects.
//!
//! ## Serde support
//!
//! [Serde] support can be enabled with a `"serde_std"` feature
//! (disabled by default) if used in `std`-enabled builds.
//!
//! This feature is called like this because of [this Cargo bug].\
//! `"serde"` feature is exists also, but it is intended to be used
//! in the `no_std` builds.
//!
//! ## No-std support
//!
//! This crate can be used in a `no_std` builds with
//! disabled `"std"` feature (enabled by default).
//!
//! Enabled `"serde"` feature will add support for `no_std`
//! serde serialization and deserialization.
//!
//! [Serde]: https://serde.rs
//! [MAC address]: https://en.wikipedia.org/wiki/MAC_address
//! [this Cargo bug]: https://github.com/rust-lang/cargo/issues/3494
//! [MacAddr6]: struct.MacAddr6.html
//! [MacAddr8]: struct.MacAddr8.html
#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.rs/macaddr/1.0.0")]
#![forbid(unsafe_code)]

mod addr;
mod addr6;
mod addr8;
mod parser;

pub use self::addr::MacAddr;
pub use self::addr6::MacAddr6;
pub use self::addr8::MacAddr8;
pub use self::parser::ParseError;
