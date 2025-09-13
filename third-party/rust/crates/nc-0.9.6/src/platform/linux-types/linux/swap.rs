// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/swap.h`

/// set if swap priority specified
pub const SWAP_FLAG_PREFER: i32 = 0x8000;
pub const SWAP_FLAG_PRIO_MASK: i32 = 0x7fff;
pub const SWAP_FLAG_PRIO_SHIFT: i32 = 0;
/// enable discard for swap
pub const SWAP_FLAG_DISCARD: i32 = 0x10000;
/// discard swap area at swapon-time
pub const SWAP_FLAG_DISCARD_ONCE: i32 = 0x20000;
/// discard page-clusters after use
pub const SWAP_FLAG_DISCARD_PAGES: i32 = 0x40000;

pub const SWAP_FLAGS_VALID: i32 = SWAP_FLAG_PRIO_MASK
    | SWAP_FLAG_PREFER
    | SWAP_FLAG_DISCARD
    | SWAP_FLAG_DISCARD_ONCE
    | SWAP_FLAG_DISCARD_PAGES;
pub const SWAP_BATCH: i32 = 64;

/// `MAX_SWAPFILES` defines the maximum number of swaptypes: things which can
/// be swapped to.
///
/// The swap type and the offset into that swap type are encoded into pte's and
/// into `pgoff_t's` in the swapcache.
///
/// Using five bits for the type means that the maximum number of swapcache pages
/// is 27 bits on `32-bit-pgoff_t` architectures.
///
/// And that assumes that the architecture packs the type/offset into the pte as 5/27 as well.
pub const MAX_SWAPFILES_SHIFT: i32 = 5;

pub const SWAP_CLUSTER_MAX: usize = 32;
pub const COMPACT_CLUSTER_MAX: usize = SWAP_CLUSTER_MAX;

/// Bit flag in `swap_map`.
///
/// Flag page is cached, in first `swap_map`
pub const SWAP_HAS_CACHE: i32 = 0x40;
/// Flag `swap_map` continuation for full count
pub const COUNT_CONTINUED: i32 = 0x80;

/// Special value in first `swap_map`.
///
/// Max count
pub const SWAP_MAP_MAX: i32 = 0x3e;
/// Note page is bad
pub const SWAP_MAP_BAD: i32 = 0x3f;
/// Owned by shmem/tmpfs
pub const SWAP_MAP_SHMEM: i32 = 0xbf;

/// Special value in each `swap_map` continuation.
///
/// Max count
pub const SWAP_CONT_MAX: i32 = 0x7f;

/// This cluster is free
pub const CLUSTER_FLAG_FREE: i32 = 1;
/// This cluster has no next cluster
pub const CLUSTER_FLAG_NEXT_NULL: i32 = 2;
/// This cluster is backing a transparent huge page
pub const CLUSTER_FLAG_HUGE: i32 = 4;

/// One swap address space for each 64M swap space
pub const SWAP_ADDRESS_SPACE_SHIFT: i32 = 14;
pub const SWAP_ADDRESS_SPACE_PAGES: i32 = 1 << SWAP_ADDRESS_SPACE_SHIFT;
