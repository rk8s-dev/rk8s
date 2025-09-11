// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm64/include/uapi/asm/signal.h`

/// Required for `AArch32` compatibility.
pub const SA_RESTORER: usize = 0x0400_0000;

pub const MINSIGSTKSZ: usize = 5120;
pub const SIGSTKSZ: usize = 16384;
