//! Pluggable storage backend implementations.
//!
//! Each backend module provides a concrete type that implements
//! [`CsiIdentity`], [`CsiController`], and [`CsiNode`].

pub mod slayerfs;
