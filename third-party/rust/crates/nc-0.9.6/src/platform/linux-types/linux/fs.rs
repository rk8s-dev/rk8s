// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/fs.h`

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct file_handle_t {
    pub handle_bytes: u32,
    pub handle_type: i32,
    /// file identifier
    pub f_handle: [u8; 0],
}

/// Umount options
/// Attempt to forcibily umount
pub const MNT_FORCE: u32 = 0x0000_0001;
/// Just detach from the tree
pub const MNT_DETACH: u32 = 0x0000_0002;
/// Mark for expiry
pub const MNT_EXPIRE: u32 = 0x0000_0004;
/// Don't follow symlink on umount
pub const UMOUNT_NOFOLLOW: u32 = 0x0000_0008;
/// Flag guaranteed to be unused
pub const UMOUNT_UNUSED: u32 = 0x8000_0000;

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::file_handle_t;

    #[test]
    fn test_size_of_file_handle() {
        assert_eq!(size_of::<file_handle_t>(), 8);
    }
}
