// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

/// Error No.
pub type Errno = i32;

/// Syscall No.
pub type Sysno = usize;

pub const MAX_ERRNO: Errno = 4095;

/// Check return value is error or not.
///
/// Returning from the syscall, a value in the range between -4095 and -1 indicates an error,
/// it is -errno.
///
/// # Errors
///
/// Returns errno if system call fails.
#[inline]
pub const fn check_errno(ret: usize) -> Result<usize, Errno> {
    #[allow(clippy::cast_possible_wrap)]
    let reti = ret as isize;
    if reti < 0 && reti >= (-MAX_ERRNO) as isize {
        #[allow(clippy::cast_possible_truncation)]
        let reti = (-reti) as Errno;
        Err(reti)
    } else {
        Ok(ret)
    }
}
