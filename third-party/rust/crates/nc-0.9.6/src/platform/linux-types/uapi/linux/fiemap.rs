// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/fiemap.h`

#![allow(clippy::module_name_repetitions)]

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct fiemap_extent_t {
    /// logical offset in bytes for the start of the extent from the beginning of the file
    pub fe_logical: u64,

    /// physical offset in bytes for the start of the extent from the beginning of the disk
    pub fe_physical: u64,

    /// length in bytes for this extent
    pub fe_length: u64,

    fe_reserved64: [u64; 2],

    /// `FIEMAP_EXTENT`_* flags for this extent
    pub fe_flags: u32,

    fe_reserved: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct fiemap_t {
    /// logical offset (inclusive) at which to start mapping (in)
    pub fm_start: u64,

    /// logical length of mapping which userspace wants (in)
    pub fm_length: u64,

    /// `FIEMAP_FLAG`_* flags for request (in/out)
    pub fm_flags: u32,

    /// number of extents that were mapped (out)
    pub fm_mapped_extents: u32,

    /// size of `fm_extents` array (in)
    pub fm_extent_count: u32,

    fm_reserved: u32,

    /// array of mapped extents (out)
    pub fm_extents: [fiemap_extent_t; 0],
}

pub const FIEMAP_MAX_OFFSET: u32 = !0;

/// sync file data before map
pub const FIEMAP_FLAG_SYNC: u32 = 0x0000_0001;
/// map extended attribute tree
pub const FIEMAP_FLAG_XATTR: u32 = 0x0000_0002;
/// request caching of the extents
pub const FIEMAP_FLAG_CACHE: u32 = 0x0000_0004;

pub const FIEMAP_FLAGS_COMPAT: u32 = FIEMAP_FLAG_SYNC | FIEMAP_FLAG_XATTR;

/// Last extent in file.
pub const FIEMAP_EXTENT_LAST: u32 = 0x0000_0001;
/// Data location unknown.
pub const FIEMAP_EXTENT_UNKNOWN: u32 = 0x0000_0002;
/// Location still pending.
///
/// Sets `EXTENT_UNKNOWN`.
pub const FIEMAP_EXTENT_DELALLOC: u32 = 0x0000_0004;
/// Data can not be read while fs is unmounted
pub const FIEMAP_EXTENT_ENCODED: u32 = 0x0000_0008;
/// Data is encrypted by fs.
///
/// Sets `EXTENT_NO_BYPASS`.
pub const FIEMAP_EXTENT_DATA_ENCRYPTED: u32 = 0x0000_0080;
/// Extent offsets may not be block aligned.
pub const FIEMAP_EXTENT_NOT_ALIGNED: u32 = 0x0000_0100;
/// Data mixed with metadata.
///
/// Sets `EXTENT_NOT_ALIGNED`.
pub const FIEMAP_EXTENT_DATA_INLINE: u32 = 0x0000_0200;
/// Multiple files in block.
///
/// Sets `EXTENT_NOT_ALIGNED`.
pub const FIEMAP_EXTENT_DATA_TAIL: u32 = 0x0000_0400;
/// Space allocated, but no data (i.e. zero).
pub const FIEMAP_EXTENT_UNWRITTEN: u32 = 0x0000_0800;
/// File does not natively support extents.
///
/// Result merged for efficiency.
pub const FIEMAP_EXTENT_MERGED: u32 = 0x0000_1000;
/// Space shared with other files.
pub const FIEMAP_EXTENT_SHARED: u32 = 0x0000_2000;
