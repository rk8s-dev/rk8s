// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub type c_char = u8;

#[cfg(target_arch = "x86_64")]
#[path = "x86_64/mod.rs"]
mod arch;
pub use arch::*;

mod bsm;
mod netinet;
mod netinet6;
mod sys;

pub use bsm::*;
pub use netinet::*;
pub use netinet6::*;
pub use sys::*;
