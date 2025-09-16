// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![cfg_attr(feature = "nightly", feature(repr_simd))]
#![deny(missing_docs)]

//! This project provides an easy interface to the famous **Boost.Context** library
//! and thus the building blocks for higher-level abstractions, like coroutines,
//! cooperative threads (userland threads) or an equivalent to the C# keyword "yield".

extern crate libc;
#[cfg(windows)]
extern crate winapi;

/// Provides the `Context` and `Transfer` types for
/// saving and restoring the current state of execution.
///
/// See the `Context` struct for more information.
pub mod context;

/// Provides utilities to allocate memory suitable as stack memory for `Context`.
pub mod stack;

mod sys;

pub use context::{Context, ContextFn, ResumeOntopFn, Transfer};

#[cfg(not(target_os = "windows"))]
pub use std::os::raw::c_void;
#[cfg(target_os = "windows")]
pub use winapi::ctypes::c_void;
