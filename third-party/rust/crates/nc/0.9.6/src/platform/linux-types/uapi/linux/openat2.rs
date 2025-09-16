// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `uapi/linux/openat2.h`

/// Arguments for how openat2(2) should open the target path. If only flags and
/// mode are non-zero, then openat2(2) operates very similarly to openat(2).
///
/// However, unlike openat(2), unknown or invalid bits in flags result in
/// `-EINVAL` rather than being silently ignored. mode must be zero unless one of
/// `{O_CREAT, O_TMPFILE}` are set.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct open_how_t {
    /// `O_*` flags
    pub flags: u64,
    /// `O_CREAT/O_TMPFILE` file mode
    pub mode: u64,
    /// `RESOLVE_*` flags.
    pub resolve: u64,
}

/// how->resolve flags for openat2(2).
///
/// Block mount-point crossings (includes bind-mounts).
pub const RESOLVE_NO_XDEV: u64 = 0x01;

/// Block traversal through procfs-style "magic-links".
pub const RESOLVE_NO_MAGICLINKS: u64 = 0x02;

/// Block traversal through all symlinks (implies `OEXT_NO_MAGICLINKS`)
pub const RESOLVE_NO_SYMLINKS: u64 = 0x04;

/// Block "lexical" trickery like "..", symlinks, and absolute paths which escape the dirfd.
pub const RESOLVE_BENEATH: u64 = 0x08;

/// Make all jumps to "/" and ".." be scoped inside the dirfd (similar to chroot(2)).
pub const RESOLVE_IN_ROOT: u64 = 0x10;

/// Only complete if resolution can be completed through cached lookup.
///
/// May return `-EAGAIN` if that's not possible.
pub const RESOLVE_CACHED: u64 = 0x20;
