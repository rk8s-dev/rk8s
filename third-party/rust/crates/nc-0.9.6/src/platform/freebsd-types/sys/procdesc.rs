// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/procdesc.h`

/// Flags which can be passed to pdfork(2).
/// Don't exit when procdesc closes.
pub const PD_DAEMON: i32 = 0x00000001;
/// Close file descriptor on exec.
pub const PD_CLOEXEC: i32 = 0x00000002;

pub const PD_ALLOWED_AT_FORK: i32 = PD_DAEMON | PD_CLOEXEC;
