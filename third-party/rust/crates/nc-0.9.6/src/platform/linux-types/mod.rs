// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_imports)]

pub type c_char = u8;

// First import architecture specific types.
#[cfg(target_arch = "aarch64")]
#[path = "aarch64/mod.rs"]
mod arch;

#[cfg(target_arch = "arm")]
#[path = "arm/mod.rs"]
mod arch;

#[cfg(target_arch = "loongarch64")]
#[path = "loongarch64/mod.rs"]
mod arch;

#[cfg(target_arch = "mips")]
#[path = "mips/mod.rs"]
mod arch;

#[cfg(target_arch = "mips64")]
#[path = "mips64/mod.rs"]
mod arch;

#[cfg(target_arch = "powerpc64")]
#[path = "ppc64/mod.rs"]
mod arch;

#[cfg(target_arch = "riscv64")]
#[path = "riscv64/mod.rs"]
mod arch;

#[cfg(target_arch = "s390x")]
#[path = "s390x/mod.rs"]
mod arch;

#[cfg(target_arch = "x86")]
#[path = "x86/mod.rs"]
mod arch;

#[cfg(target_arch = "x86_64")]
#[path = "x86_64/mod.rs"]
mod arch;

pub use arch::*;

mod asm_generic;
mod basic_types;
mod fs;
mod linux;
mod uapi;

pub use asm_generic::*;
pub use basic_types::*;
pub use fs::*;
pub use linux::*;
pub use uapi::*;
