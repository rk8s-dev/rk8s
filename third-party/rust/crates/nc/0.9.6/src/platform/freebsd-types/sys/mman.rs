// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/mman.h`

/// Inheritance for `minherit()`
pub const INHERIT_SHARE: i32 = 0;
pub const INHERIT_COPY: i32 = 1;
pub const INHERIT_NONE: i32 = 2;
pub const INHERIT_ZERO: i32 = 3;

/// Protections are chosen from these bits, or-ed together
/// no permissions
pub const PROT_NONE: i32 = 0x00;
/// pages can be read
pub const PROT_READ: i32 = 0x01;
/// pages can be written
pub const PROT_WRITE: i32 = 0x02;
/// pages can be executed
pub const PROT_EXEC: i32 = 0x04;
pub const _PROT_ALL: i32 = PROT_READ | PROT_WRITE | PROT_EXEC;

#[must_use]
pub const fn PROT_EXTRACT(prot: i32) -> i32 {
    prot & _PROT_ALL
}

pub const _PROT_MAX_SHIFT: i32 = 16;

#[must_use]
pub const fn PROT_MAX(prot: i32) -> i32 {
    prot << _PROT_MAX_SHIFT
}

#[must_use]
pub const fn PROT_MAX_EXTRACT(prot: i32) -> i32 {
    (prot >> _PROT_MAX_SHIFT) & _PROT_ALL
}

/// Flags contain sharing type and options.
///
/// Sharing types; choose one.
///
/// share changes
pub const MAP_SHARED: i32 = 0x0001;
/// changes are private
pub const MAP_PRIVATE: i32 = 0x0002;
/// Obsolete
pub const MAP_COPY: i32 = MAP_PRIVATE;

/// Other flags
/// map addr must be exactly as requested
pub const MAP_FIXED: i32 = 0x0010;

/// previously unimplemented `MAP_RENAME`
pub const MAP_RESERVED0020: i32 = 0x0020;
/// previously unimplemented `MAP_NORESERVE`
pub const MAP_RESERVED0040: i32 = 0x0040;
/// previously misimplemented `MAP_INHERIT`
pub const MAP_RESERVED0080: i32 = 0x0080;
/// previously unimplemented `MAP_NOEXTEND`
pub const MAP_RESERVED0100: i32 = 0x0100;
/// region may contain semaphores
pub const MAP_HASSEMAPHORE: i32 = 0x0200;
/// region grows down, like a stack
pub const MAP_STACK: i32 = 0x0400;
/// page to but do not sync underlying file
pub const MAP_NOSYNC: i32 = 0x0800;

/// Mapping type
/// map from file (default)
pub const MAP_FILE: i32 = 0x0000;
/// allocated from memory, swap space
pub const MAP_ANON: i32 = 0x1000;
/// For compatibility.
pub const MAP_ANONYMOUS: i32 = MAP_ANON;

/// Extended flags
/// reserve but don't map address range
pub const MAP_GUARD: i32 = 0x0000_2000;
/// for `MAP_FIXED`, fail if address is used
pub const MAP_EXCL: i32 = 0x0000_4000;
/// dont include these pages in a coredump
pub const MAP_NOCORE: i32 = 0x0002_0000;
/// prefault mapping for reading
pub const MAP_PREFAULT_READ: i32 = 0x0004_0000;

#[cfg(target_pointer_width = "64")]
/// map in the low 2GB of address space
pub const MAP_32BIT: i32 = 0x0008_0000;

/// Request specific alignment (n == log2 of the desired alignment).
///
/// `MAP_ALIGNED_SUPER` requests optimal superpage alignment, but does
/// not enforce a specific alignment.
#[must_use]
pub const fn MAP_ALIGNED(n: i32) -> i32 {
    n << MAP_ALIGNMENT_SHIFT
}

pub const MAP_ALIGNMENT_SHIFT: i32 = 24;
pub const MAP_ALIGNMENT_MASK: i32 = MAP_ALIGNED(0xff);
/// align on a superpage
pub const MAP_ALIGNED_SUPER: i32 = MAP_ALIGNED(1);

/// Flags provided to `shm_rename`
/// Don't overwrite dest, if it exists
pub const SHM_RENAME_NOREPLACE: i32 = 1 << 0;
/// Atomically swap src and dest
pub const SHM_RENAME_EXCHANGE: i32 = 1 << 1;

/// Process memory locking
/// Lock only current memory
pub const MCL_CURRENT: i32 = 0x0001;
/// Lock all future memory as well
pub const MCL_FUTURE: i32 = 0x0002;

/// Error return from `mmap()`
#[allow(clippy::cast_sign_loss)]
pub const MAP_FAILED: usize = -1_isize as usize;

/// `msync()` flags
/// msync synchronously
pub const MS_SYNC: i32 = 0x0000;
/// return immediately
pub const MS_ASYNC: i32 = 0x0001;
/// invalidate all cached data
pub const MS_INVALIDATE: i32 = 0x0002;

/// Advice to madvise
/// no further special treatment
pub const _MADV_NORMAL: i32 = 0;
/// expect random page references
pub const _MADV_RANDOM: i32 = 1;
/// expect sequential page references
pub const _MADV_SEQUENTIAL: i32 = 2;
/// will need these pages
pub const _MADV_WILLNEED: i32 = 3;
/// dont need these pages
pub const _MADV_DONTNEED: i32 = 4;

pub const MADV_NORMAL: i32 = _MADV_NORMAL;
pub const MADV_RANDOM: i32 = _MADV_RANDOM;
pub const MADV_SEQUENTIAL: i32 = _MADV_SEQUENTIAL;
pub const MADV_WILLNEED: i32 = _MADV_WILLNEED;
pub const MADV_DONTNEED: i32 = _MADV_DONTNEED;
/// dont need these pages, and junk contents
pub const MADV_FREE: i32 = 5;
/// try to avoid flushes to physical media
pub const MADV_NOSYNC: i32 = 6;
/// revert to default flushing strategy
pub const MADV_AUTOSYNC: i32 = 7;
/// do not include these pages in a core file
pub const MADV_NOCORE: i32 = 8;
/// revert to including pages in a core file
pub const MADV_CORE: i32 = 9;
/// protect process from pageout kill
pub const MADV_PROTECT: i32 = 10;

/// Return bits from mincore
/// Page is incore
pub const MINCORE_INCORE: i32 = 0x1;
/// Page has been referenced by us
pub const MINCORE_REFERENCED: i32 = 0x2;
/// Page has been modified by us
pub const MINCORE_MODIFIED: i32 = 0x4;
/// Page has been referenced
pub const MINCORE_REFERENCED_OTHER: i32 = 0x8;
/// Page has been modified
pub const MINCORE_MODIFIED_OTHER: i32 = 0x10;
/// Page is a "super" page
pub const MINCORE_SUPER: i32 = 0x60;
/// Page size
#[must_use]
pub const fn MINCORE_PSIND(i: i32) -> i32 {
    (i << 5) & MINCORE_SUPER
}

/// Anonymous object constant for `shm_open()`.
pub const SHM_ANON: usize = 1;

/// shmflags for `shm_open2()`
pub const SHM_ALLOW_SEALING: i32 = 0x0000_0001;
pub const SHM_GROW_ON_WRITE: i32 = 0x0000_0002;
pub const SHM_LARGEPAGE: i32 = 0x0000_0004;

pub const SHM_LARGEPAGE_ALLOC_DEFAULT: i32 = 0;
pub const SHM_LARGEPAGE_ALLOC_NOWAIT: i32 = 1;
pub const SHM_LARGEPAGE_ALLOC_HARD: i32 = 2;

/// Flags for `memfd_create()`.
pub const MFD_CLOEXEC: u32 = 0x0000_0001;
pub const MFD_ALLOW_SEALING: u32 = 0x0000_0002;

pub const MFD_HUGETLB: u32 = 0x0000_0004;

pub const MFD_HUGE_MASK: u32 = 0xFC00_0000;
pub const MFD_HUGE_SHIFT: u32 = 26;
pub const MFD_HUGE_64KB: usize = 16 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_512KB: usize = 19 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_1MB: usize = 20 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_2MB: usize = 21 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_8MB: usize = 23 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_16MB: usize = 24 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_32MB: usize = 25 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_256MB: usize = 28 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_512MB: usize = 29 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_1GB: usize = 30 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_2GB: usize = 31 << MFD_HUGE_SHIFT;
pub const MFD_HUGE_16GB: usize = 34 << MFD_HUGE_SHIFT;

pub const POSIX_MADV_NORMAL: i32 = _MADV_NORMAL;
pub const POSIX_MADV_RANDOM: i32 = _MADV_RANDOM;
pub const POSIX_MADV_SEQUENTIAL: i32 = _MADV_SEQUENTIAL;
pub const POSIX_MADV_WILLNEED: i32 = _MADV_WILLNEED;
pub const POSIX_MADV_DONTNEED: i32 = _MADV_DONTNEED;
