// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[cfg(any(
    target_arch = "aarch64",
    target_arch = "loongarch64",
    target_arch = "riscv64"
))]
mod signal;
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "loongarch64",
    target_arch = "riscv64"
))]
pub use signal::*;

#[cfg(any(
    target_arch = "aarch64",
    target_arch = "loongarch64",
    target_arch = "riscv64"
))]
mod stat;
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "loongarch64",
    target_arch = "riscv64"
))]
pub use stat::*;

mod fcntl;
mod hugetlb_encode;
mod ioctl;
mod ioctls;
mod ipcbuf;
mod mman;
mod mman_common;
mod msgbuf;
mod poll;
mod posix_types;
mod resource;
mod shmbuf;
mod siginfo;
mod signal_defs;
mod socket;
mod sockios;
mod statfs;
mod termbits;
mod termbits_common;
mod termios;

pub use fcntl::*;
pub use hugetlb_encode::*;
pub use ioctl::*;
pub use ioctls::*;
pub use ipcbuf::*;
pub use mman::*;
pub use mman_common::*;
pub use msgbuf::*;
pub use poll::*;
pub use posix_types::*;
pub use resource::*;
pub use shmbuf::*;
pub use siginfo::*;
pub use signal_defs::*;
pub use socket::*;
pub use sockios::*;
pub use statfs::*;
pub use termbits::*;
pub use termbits_common::*;
pub use termios::*;
