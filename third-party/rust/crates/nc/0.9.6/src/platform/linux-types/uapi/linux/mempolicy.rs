// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/mempolicy.h`

//! NUMA memory policies for Linux.
//!
//! Both the `MPOL_*` mempolicy mode and the `MPOL_F_*` optional mode flags are
//! passed by the user to either `set_mempolicy()` or `mbind()` in an 'int' actual.
//! The `MPOL_MODE_FLAGS` macro determines the legal set of optional mode flags.

// NOTE(Shaohua): Types of flags are i32 in kernel, but they are migrated to u32 here.

/// Policies
pub const MPOL_DEFAULT: u32 = 0;
pub const MPOL_PREFERRED: u32 = 1;
pub const MPOL_BIND: u32 = 2;
pub const MPOL_INTERLEAVE: u32 = 3;
pub const MPOL_LOCAL: u32 = 4;
/// always last member of enum
pub const MPOL_MAX: u32 = 5;

/// Flags for `set_mempolicy`
pub const MPOL_F_STATIC_NODES: u32 = 1 << 15;
pub const MPOL_F_RELATIVE_NODES: u32 = 1 << 14;

/// `MPOL_MODE_FLAGS` is the union of all possible optional mode flags passed to
/// either `set_mempolicy()` or `mbind()`.
pub const MPOL_MODE_FLAGS: u32 = MPOL_F_STATIC_NODES | MPOL_F_RELATIVE_NODES;

/// Flags for `get_mempolicy`
/// return next IL mode instead of node mask
pub const MPOL_F_NODE: u32 = 1;
/// look up vma using address
pub const MPOL_F_ADDR: u32 = 1 << 1;
/// return allowed memories
pub const MPOL_F_MEMS_ALLOWED: u32 = 1 << 2;

/// Flags for mbind
/// Verify existing pages in the mapping
pub const MPOL_MF_STRICT: u32 = 1;

/// Move pages owned by this process to conform to policy
pub const MPOL_MF_MOVE: u32 = 1 << 1;
/// Move every page to conform to policy
pub const MPOL_MF_MOVE_ALL: u32 = 1 << 2;
/// Modifies '_MOVE:  lazy migrate on fault
pub const MPOL_MF_LAZY: u32 = 1 << 3;
/// Internal flags start here
pub const MPOL_MF_INTERNAL: u32 = 1 << 4;

pub const MPOL_MF_VALID: u32 = MPOL_MF_STRICT | MPOL_MF_MOVE | MPOL_MF_MOVE_ALL;

/// Internal flags that share the struct mempolicy flags word with
/// "mode flags".
///
/// These flags are allocated from bit 0 up, as they are never OR'ed into the mode
///
/// in mempolicy API arguments.
///
/// identify shared policies
pub const MPOL_F_SHARED: u32 = 1;
/// preferred local allocation
pub const MPOL_F_LOCAL: u32 = 1 << 1;
/// this policy wants migrate on fault
pub const MPOL_F_MOF: u32 = 1 << 3;
/// Migrate On protnone Reference On Node
pub const MPOL_F_MORON: u32 = 1 << 4;
