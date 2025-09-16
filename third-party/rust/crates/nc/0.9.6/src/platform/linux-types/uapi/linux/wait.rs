// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/wait.h`

pub const WNOHANG: i32 = 0x0000_0001;
pub const WUNTRACED: i32 = 0x0000_0002;
pub const WSTOPPED: i32 = WUNTRACED;
pub const WEXITED: i32 = 0x0000_0004;
pub const WCONTINUED: i32 = 0x0000_0008;
/// Don't reap, just poll status.
pub const WNOWAIT: i32 = 0x0100_0000;

/// Don't wait on children of other threads in this group
pub const __WNOTHREAD: i32 = 0x2000_0000;
/// Wait on all children, regardless of type
pub const __WALL: i32 = 0x4000_0000;
/// Wait only on non-SIGCHLD children
#[allow(overflowing_literals)]
pub const __WCLONE: i32 = 0x8000_0000;

/// First argument to waitid:
pub const P_ALL: i32 = 0;
pub const P_PID: i32 = 1;
pub const P_PGID: i32 = 2;
pub const P_PIDFD: i32 = 3;
