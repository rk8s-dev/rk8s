//! QUIC transport layer for CSI messages.
//!
//! This module provides [`CsiClient`] and [`CsiServer`] that communicate
//! [`CsiMessage`] values over QUIC bi-directional streams using `quinn`.

pub mod client;
pub mod server;
