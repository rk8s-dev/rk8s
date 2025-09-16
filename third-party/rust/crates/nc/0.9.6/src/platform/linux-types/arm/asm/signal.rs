// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// found in the LICENSE file.

//! From `arch/arm/include/asm/signal.h`

pub const _NSIG: usize = 64;
pub const _NSIG_BPW: usize = 32;
pub const _NSIG_WORDS: usize = _NSIG / _NSIG_BPW;

/// at least 32 bits
pub type old_sigset_t = usize;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigset_t {
    pub sig: [usize; _NSIG_WORDS],
}
