// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::module_name_repetitions)]

use crate::{sigaction_t, sighandler_t, SA_RESTART};

#[cfg(nc_has_sa_restorer)]
#[inline]
pub fn new_sigaction(handler: fn(i32)) -> sigaction_t {
    sigaction_t {
        sa_handler: handler as sighandler_t,
        sa_flags: SA_RESTART | crate::SA_RESTORER,
        sa_restorer: crate::restore::get_sa_restorer(),
        ..sigaction_t::default()
    }
}

#[cfg(not(nc_has_sa_restorer))]
#[inline]
pub fn new_sigaction(handler: fn(i32)) -> sigaction_t {
    sigaction_t {
        sa_handler: handler as sighandler_t,
        sa_flags: SA_RESTART,
        ..sigaction_t::default()
    }
}
