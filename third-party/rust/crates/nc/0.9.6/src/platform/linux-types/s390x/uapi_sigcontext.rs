// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/s390/include/uapi/asm/sigcontext.h`

use core::mem::size_of;

pub const __NUM_GPRS: usize = 16;
pub const __NUM_FPRS: usize = 16;
pub const __NUM_ACRS: usize = 16;
pub const __NUM_VXRS: usize = 32;
pub const __NUM_VXRS_LOW: usize = 16;
pub const __NUM_VXRS_HIGH: usize = 16;

// Has to be at least _NSIG_WORDS from asm/signal.h
pub const _SIGCONTEXT_NSIG: usize = 64;
pub const _SIGCONTEXT_NSIG_BPW: usize = 64;
// Size of stack frame allocated when calling signal handler.
pub const __SIGNAL_FRAMESIZE: usize = 160;

pub const _SIGCONTEXT_NSIG_WORDS: usize = _SIGCONTEXT_NSIG / _SIGCONTEXT_NSIG_BPW;
pub const _SIGMASK_COPY_SIZE: usize = size_of::<usize>() * _SIGCONTEXT_NSIG_WORDS;
