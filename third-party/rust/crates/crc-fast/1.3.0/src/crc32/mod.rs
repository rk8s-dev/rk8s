// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! This module provides CRC-32 support.

pub mod algorithm;
pub mod consts;

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub(crate) mod fusion;
