// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `i386/param.h`
//! Machine dependent constants for Intel 386.

/// bytes/page
pub const NBPG: usize = 4096;
/// byte offset into page
pub const PGOFSET: usize = NBPG - 1;
/// LOG2(NBPG)
pub const PGSHIFT: i32 = 12;

pub const DEV_BSIZE: i32 = 512;
/// log2(DEV_BSIZE)
pub const DEV_BSHIFT: i32 = 9;
pub const BLKDEV_IOSIZE: i32 = 2048;
/// max raw I/O transfer size
pub const MAXPHYS: usize = 128 * 1024;

pub const CLSIZE: i32 = 1;
pub const CLSIZELOG2: i32 = 0;

/// Constants related to network buffer management.
///
/// MCLBYTES must be no larger than CLBYTES (the software page size), and,
/// on machines that exchange pages of input or output buffers with mbuf
/// clusters (MAPPED_MBUFS), MCLBYTES must also be an integral multiple
/// of the hardware page size.
///
/// 256
pub const MSIZESHIFT: i32 = 8;
/// size of an mbuf
pub const MSIZE: usize = 1 << MSIZESHIFT;
/// 2048
pub const MCLSHIFT: i32 = 11;
/// size of an mbuf cluster
pub const MCLBYTES: usize = 1 << MCLSHIFT;
/// 4096
pub const MBIGCLSHIFT: i32 = 12;
/// size of a big cluster
pub const MBIGCLBYTES: usize = 1 << MBIGCLSHIFT;
/// 16384
pub const M16KCLSHIFT: i32 = 14;
/// size of a jumbo cluster
pub const M16KCLBYTES: usize = 1 << M16KCLSHIFT;
