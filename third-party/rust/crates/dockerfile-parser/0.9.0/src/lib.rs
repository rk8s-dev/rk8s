// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

#![forbid(unsafe_code)]

//! # Rust parser for Dockerfile syntax
//!
//! A pure Rust library for parsing and inspecting Dockerfiles, useful for
//! performing static analysis, writing linters, and creating automated tooling
//! around Dockerfiles. It can provide useful syntax errors in addition to a
//! full syntax tree.
//!
//! ## Quick start
//!
//! ```rust
//! use dockerfile_parser::Dockerfile;
//!
//! let dockerfile = Dockerfile::parse(r#"
//!   FROM alpine:3.11 as builder
//!   RUN echo "hello world" > /hello-world
//!
//!   FROM scratch
//!   COPY --from=builder /hello-world /hello-world
//! "#).unwrap();
//!
//! for stage in dockerfile.iter_stages() {
//!   println!("stage #{}", stage.index);
//!   for ins in stage.instructions {
//!     println!("  {:?}", ins);
//!   }
//! }
//! ```

#[macro_use] extern crate pest_derive;

mod error;
mod parser;
mod util;
mod image;
mod instructions;
mod splicer;
mod stage;
mod dockerfile_parser;

pub use image::*;
pub use error::*;
pub use parser::*;
pub use instructions::*;
pub use splicer::*;
pub use stage::*;
pub use util::*;
pub use crate::dockerfile_parser::*;

#[cfg(test)] mod test_util;
