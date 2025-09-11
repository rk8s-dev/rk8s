// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

use crate::{itimerval_t, setitimer, Errno, ITIMER_REAL};

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub unsafe fn alarm(seconds: u32) -> Result<u32, Errno> {
    let mut it = itimerval_t::default();
    it.it_value.tv_sec = seconds as isize;
    let mut old = itimerval_t::default();
    setitimer(ITIMER_REAL, &it, Some(&mut old))?;
    let remaining = (old.it_value.tv_sec + !!old.it_value.tv_usec) as u32;
    Ok(remaining)
}
