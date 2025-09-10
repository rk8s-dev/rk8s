// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `amd64/include/param.h`

use core::mem::size_of;

use crate::{pd_entry_t, pdp_entry_t, pml4_entry_t, pml5_entry_t, pt_entry_t};

/// Machine dependent constants for AMD64.
pub const MAXCPU: i32 = 256;

pub const MAXMEMDOM: i32 = 8;

/// `CACHE_LINE_SIZE` is the compile-time maximum cache line size for an
/// architecture.  It should be used with appropriate caution.
pub const CACHE_LINE_SHIFT: i32 = 6;
pub const CACHE_LINE_SIZE: usize = 1 << CACHE_LINE_SHIFT;

/// Size of the level 1 page table units
pub const NPTEPG: usize = PAGE_SIZE / size_of::<pt_entry_t>();

/// LOG2(NPTEPG)
pub const NPTEPGSHIFT: i32 = 9;
/// `LOG2(PAGE_SIZE)`
pub const PAGE_SHIFT: i32 = 12;
/// bytes/page
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = PAGE_SIZE - 1;
/// Size of the level 2 page directory units
pub const NPDEPG: usize = PAGE_SIZE / size_of::<pd_entry_t>();
/// LOG2(NPDEPG)
pub const NPDEPGSHIFT: i32 = 9;
/// LOG2(NBPDR)
pub const PDRSHIFT: i32 = 21;
/// bytes/page dir
pub const NBPDR: usize = 1 << PDRSHIFT;
pub const PDRMASK: usize = NBPDR - 1;

/// Size of the level 3 page directory pointer table units
pub const NPDPEPG: usize = PAGE_SIZE / size_of::<pdp_entry_t>();
/// LOG2(NPDPEPG)
pub const NPDPEPGSHIFT: i32 = 9;
/// LOG2(NBPDP)
pub const PDPSHIFT: i32 = 30;
/// bytes/page dir ptr table
pub const NBPDP: usize = 1 << PDPSHIFT;
pub const PDPMASK: usize = NBPDP - 1;

/// Size of the level 4 page-map level-4 table units
pub const NPML4EPG: usize = PAGE_SIZE / size_of::<pml4_entry_t>();
/// LOG2(NPML4EPG)
pub const NPML4EPGSHIFT: i32 = 9;
/// LOG2(NBPML4)
pub const PML4SHIFT: i32 = 39;
/// bytes/page map lev4 table
pub const NBPML4: usize = 1 << PML4SHIFT;
pub const PML4MASK: usize = NBPML4 - 1;

/// Size of the level 5 page-map level-5 table units
pub const NPML5EPG: usize = PAGE_SIZE / size_of::<pml5_entry_t>();
/// LOG2(NPML5EPG)
pub const NPML5EPGSHIFT: i32 = 9;
/// LOG2(NBPML5)
pub const PML5SHIFT: i32 = 48;
/// bytes/page map lev5 table
pub const NBPML5: usize = 1 << PML5SHIFT;
pub const PML5MASK: usize = NBPML5 - 1;

/// maximum number of supported page sizes
pub const MAXPAGESIZES: usize = 3;

/// pages of i/o permission bitmap
pub const IOPAGES: usize = 2;

/// I/O permission bitmap has a bit for each I/O port plus an additional
/// byte at the end with all bits set. See section "I/O Permission Bit Map"
/// in the Intel SDM for more details.
pub const IOPERM_BITMAP_SIZE: usize = IOPAGES * PAGE_SIZE + 1;

/// pages of kstack (with pcb)
pub const KSTACK_PAGES: i32 = 4;

/// pages of kstack guard; 0 disables
pub const KSTACK_GUARD_PAGES: i32 = 1;

/// Must be power of 2.
pub const SC_TABLESIZE: i32 = 1024;
