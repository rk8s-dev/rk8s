// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[cfg(all(
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "riscv64")
))]
mod fork;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "riscv64")
))]
pub use fork::fork;

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "loongarch64",
        target_arch = "riscv64",
    )
))]
mod alarm;

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "loongarch64",
        target_arch = "riscv64",
    )
))]
pub use alarm::alarm;

mod sigaction;
pub use sigaction::new_sigaction;
