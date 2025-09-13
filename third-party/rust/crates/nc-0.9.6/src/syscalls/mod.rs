// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

mod types;
pub use types::*;

#[cfg(not(nc_has_asm))]
#[path = "c.rs"]
mod syscall;

#[cfg(target_arch = "aarch64")]
#[path = "syscall_aarch64.rs"]
mod syscall;

#[cfg(target_arch = "arm")]
#[path = "syscall_arm.rs"]
mod syscall;

#[cfg(all(nc_has_asm, target_arch = "mips"))]
#[path = "syscall_mips.rs"]
mod syscall;

#[cfg(all(nc_has_asm, target_arch = "mips64"))]
#[path = "syscall_mips64.rs"]
mod syscall;

#[cfg(all(nc_has_asm, target_arch = "powerpc64"))]
#[path = "syscall_powerpc64.rs"]
mod syscall;

#[cfg(target_arch = "riscv64")]
#[path = "syscall_riscv64.rs"]
mod syscall;

#[cfg(all(nc_has_asm, target_arch = "s390x"))]
#[path = "syscall_s390x.rs"]
mod syscall;

#[cfg(target_arch = "x86")]
#[path = "syscall_x86.rs"]
mod syscall;

#[cfg(target_arch = "x86_64")]
#[path = "syscall_x86_64.rs"]
mod syscall;

pub use syscall::*;
