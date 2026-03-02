//! QUIC transport implementation for Curp RPC
//!
//! This module provides QUIC-based transport as an alternative to tonic gRPC.
//! It implements the same `ConnectApi`/`InnerConnectApi` traits but uses
//! gm-quic streams with prost encoding instead of tonic channels.

pub(crate) mod channel;
pub(crate) mod codec;
pub(crate) mod server;

pub use channel::DnsFallback;
pub use channel::QuicChannel;
pub use codec::MethodId;
pub use server::QuicGrpcServer;

#[doc(hidden)]
#[cfg(any(test, feature = "quic-test"))]
pub use codec::ALL_METHOD_IDS;
