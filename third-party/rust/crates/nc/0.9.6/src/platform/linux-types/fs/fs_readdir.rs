// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `fs/readir.c`

use core::{fmt, ptr, slice};

use crate::c_str::strlen;
use crate::{ino64_t, loff_t, DT_UNKNOWN, PATH_MAX};

#[repr(C)]
#[derive(Default)]
pub struct linux_dirent_t {
    /// Inode number
    pub d_ino: ino64_t,

    /// Offset to next `linux_dirent`
    pub d_off: loff_t,

    /// Length of this `linux_dirent`
    pub d_reclen: u16,

    /// Filename (null-terminated)
    pub d_name: [u8; 0],
}

impl linux_dirent_t {
    /// Read `d_type` from struct.
    ///
    /// `d_type` located at last byte of dirent.
    #[must_use]
    #[inline]
    pub fn d_type(&self) -> u8 {
        let self_ptr: *const u8 = (self as *const Self).cast::<u8>();
        let d_type_ptr: *const u8 = self_ptr
            .wrapping_add(self.d_reclen as usize)
            .wrapping_sub(1);
        unsafe { d_type_ptr.read() }
    }

    /// Get inner `CString`
    #[must_use]
    #[inline]
    pub fn name(&self) -> &[u8] {
        let d_name_ptr: *const u8 = ptr::addr_of!(self.d_name).cast::<u8>();
        let name_len = strlen(d_name_ptr as usize, self.d_reclen as usize);
        unsafe { slice::from_raw_parts(d_name_ptr, name_len) }
    }
}

impl fmt::Debug for linux_dirent_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("linux_dirent_t")
            .field("d_ino", &self.d_ino)
            .field("d_off", &self.d_off)
            .field("d_reclen", &self.d_reclen)
            .field("d_name", &self.name())
            .finish()
    }
}
