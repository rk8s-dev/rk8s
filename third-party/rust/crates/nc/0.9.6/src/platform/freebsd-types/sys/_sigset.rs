// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_sigset.h`

pub const _SIG_WORDS: usize = 4;
pub const _SIG_MAXSIG: i32 = 128;

#[must_use]
pub const fn _SIG_IDX(sig: i32) -> i32 {
    sig - 1
}

#[must_use]
pub const fn _SIG_WORD(sig: i32) -> usize {
    (_SIG_IDX(sig) >> 5) as usize
}

#[must_use]
pub const fn _SIG_BIT(sig: i32) -> u32 {
    (1 << (_SIG_IDX(sig) & 31)) as u32
}

#[must_use]
pub const fn _SIG_VALID(sig: i32) -> bool {
    sig <= _SIG_MAXSIG && sig > 0
}

/// `sigset_t` macros.
#[repr(C)]
#[derive(Debug, Default, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct sigset_t {
    pub bits: [u32; _SIG_WORDS],
}

pub type osigset_t = u32;

/// Alias to `osigset_t`
pub type old_sigset_t = osigset_t;
