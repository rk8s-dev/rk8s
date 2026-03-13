//! QUIC transport implementation for Curp RPC
//!
//! This module provides QUIC-based transport for Curp.
//! It implements the `ConnectApi`/`InnerConnectApi` traits using
//! gm-quic streams with prost encoding.

pub(crate) mod channel;
pub(crate) mod codec;
pub(crate) mod server;

pub use channel::DnsFallback;
pub use channel::QuicChannel;
pub use codec::MethodId;
pub use server::QuicGrpcServer;

#[doc(hidden)]
pub use codec::ALL_METHOD_IDS;
