// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(non_camel_case_types)]

pub type c_char = i8;

#[cfg(target_arch = "x86_64")]
#[path = "i386/mod.rs"]
mod arch;
pub use arch::*;

#[cfg(target_arch = "aarch64")]
#[path = "arm/mod.rs"]
mod arch;
pub use arch::*;

mod bsm;
mod copyfile;
mod netinet;
mod sys;

pub use bsm::*;
pub use copyfile::*;
pub use netinet::*;
pub use sys::*;
