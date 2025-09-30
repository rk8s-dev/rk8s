//! This is a Rust module that contains several adaptors to different cryptography libraries.
//! The rusty_vault::crypto module utilize these adaptors to do the real crypto operations.
//!
//! Only one crypto adaptor can be used in one build. It's configured when building RustyVault.
//! An adaptor implements a set of methods that perform cryptograhpy operations like encryption,
//! decription, signing, verification and so on.

#[macro_use]
pub mod common;
#[cfg(feature = "crypto_adaptor_openssl")]
pub mod openssl_adaptor;
#[cfg(feature = "crypto_adaptor_tongsuo")]
pub mod tongsuo_adaptor;
