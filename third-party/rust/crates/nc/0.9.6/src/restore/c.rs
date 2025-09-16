// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[cfg(nc_has_sa_restorer)]
use crate::{restorefn_t, rt_sigaction, sigaction_t, SIGSEGV};

/// Reuse sa restorer function.
///
/// This method is unreliable.
#[cfg(nc_has_sa_restorer)]
#[must_use]
#[inline]
pub fn get_sa_restorer() -> Option<restorefn_t> {
    let mut old_sa = sigaction_t::default();
    let ret = unsafe { rt_sigaction(SIGSEGV, None, Some(&mut old_sa)) };
    if ret.is_ok() {
        old_sa.sa_restorer
    } else {
        None
    }
}

#[cfg(not(nc_has_sa_restorer))]
#[must_use]
#[inline]
pub fn get_sa_restorer() -> Option<crate::restorefn_t> {
    None
}
