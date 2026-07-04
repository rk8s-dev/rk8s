//! Rust-friendly OCI image primitives for rk8s.

pub mod descriptor;
pub mod image_ref;
pub mod media_type;
pub mod mirror;
pub mod platform;

pub use descriptor::{Descriptor, Digest};
pub use image_ref::{ImageReference, ReferenceKind};
pub use media_type::MediaType;
pub use mirror::RegistryMirrorConfig;
pub use platform::Platform;

/// Result type used by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for OCI reference and metadata handling.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Input was empty or otherwise malformed.
    #[error("invalid OCI value: {0}")]
    Invalid(String),
}
