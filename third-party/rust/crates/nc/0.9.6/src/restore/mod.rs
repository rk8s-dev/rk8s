// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
    target_arch = "x86",
    target_arch = "x86_64",
))]
use core::arch::global_asm;

#[cfg(not(any(
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
    target_arch = "x86",
    target_arch = "x86_64",
)))]
#[path = "c.rs"]
mod imp;

#[cfg(target_arch = "aarch64")]
global_asm!(include_str!("restore_aarch64.s"));

#[cfg(target_arch = "arm")]
global_asm!(include_str!("restore_arm.s"));

#[cfg(target_arch = "riscv64")]
global_asm!(include_str!("restore_riscv64.s"));

#[cfg(target_arch = "x86")]
global_asm!(include_str!("restore_x86.s"));

#[cfg(target_arch = "x86_64")]
global_asm!(include_str!("restore_x86_64.s"));

#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm",
    target_arch = "riscv64",
    target_arch = "x86",
    target_arch = "x86_64",
))]
#[path = "native.rs"]
mod imp;

pub use imp::*;
