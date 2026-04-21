//! HTTP/3 server implementation for xlinerpc
//!
//! This module provides HTTP/3 server-side support for gRPC services,
//! complementary to the client-side support in `h3_client`.

pub mod endpoint;
pub mod h3wrapper;
pub mod server;

pub use self::endpoint::{Router, StateRouter};
pub use endpoint::EndPoint;
pub use h3wrapper::{
    MakeClientStreamingSvc, MakeServerStreamingSvc, MakeStreamingSvc, MakeUnarySVC,
    QuicIncomingBody,
};
pub use server::{extract_host_from_url, extract_ports_from_urls, parse_bind_uri};
