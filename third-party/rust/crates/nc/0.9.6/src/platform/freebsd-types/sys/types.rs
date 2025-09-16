// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/types.h`

/// quads (deprecated).
pub type u_quad_t = u64;

pub type quad_t = i64;
pub type qaddr_t = *mut quad_t;

/// core address
pub type caddr_t = *mut u8;

/// core address, pointer to const
pub type c_caddr_t = *const u8;

/// base type for internet address
pub type in_addr_t = u32;

pub type in_port_t = u16;

pub type sbintime_t = i64;

/// Types suitable for exporting physical addresses, virtual addresses
/// (pointers), and memory object sizes from the kernel independent of native
/// word size.  These should be used in place of `vm_paddr_t`, `(u)intptr_t`,
/// and `size_t` in structs which contain such types that are shared with userspace.
pub type kpaddr_t = u64;
pub type kvaddr_t = u64;
pub type ksize_t = u64;
pub type kssize_t = i64;

pub type vm_ooffset_t = u64;
pub type vm_pindex_t = u64;

/// Interrupt mask (spl, `xxx_imask`...).
pub type intrmask_t = u32;

pub type uoff_t = u64;

/// memory attribute codes
pub type vm_memattr_t = u8;
