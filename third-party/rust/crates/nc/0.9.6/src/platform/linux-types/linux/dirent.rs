// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/dirent.h`

#![allow(clippy::module_name_repetitions)]

use core::{fmt, ptr, slice};

use crate::c_str::strlen;
use crate::{ino64_t, loff_t};

const NAME_MAX_LEN: usize = 256;

#[repr(C)]
pub struct linux_dirent64_t {
    /// 64-bit inode number.
    pub d_ino: ino64_t,

    /// 64-bit offset to next structure.
    pub d_off: loff_t,

    /// Size of this dirent.
    pub d_reclen: u16,

    /// File type.
    pub d_type: u8,

    /// Filename (null-terminated).
    pub d_name: [u8; NAME_MAX_LEN],
}

impl Default for linux_dirent64_t {
    fn default() -> Self {
        Self {
            d_ino: 0,
            d_off: 0,
            d_reclen: 0,
            d_type: 0,
            d_name: [0; NAME_MAX_LEN],
        }
    }
}

impl linux_dirent64_t {
    /// Return filename.
    ///
    /// name does not contain null-termination.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &[u8] {
        let name_len = strlen(self.d_name.as_ptr() as usize, self.d_reclen as usize);
        &self.d_name[..name_len]
    }
}

impl fmt::Debug for linux_dirent64_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("linux_dirent64_t")
            .field("d_ino", &self.d_ino)
            .field("d_off", &self.d_off)
            .field("d_reclen", &self.d_reclen)
            .field("d_type", &self.d_type)
            .field("d_name", &&self.d_name[..32])
            .finish()
    }
}
