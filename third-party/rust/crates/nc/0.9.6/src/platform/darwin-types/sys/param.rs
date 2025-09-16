// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/param.h`

#![allow(non_upper_case_globals)]

use core::mem::size_of;

use crate::{dev_t, gid_t, ARG_MAX, CHILD_MAX, MAXPHYS, NGROUPS_MAX, PATH_MAX};

/// System version (year & month).
pub const BSD: i32 = 199506;
pub const BSD4_3: i32 = 1;
pub const BSD4_4: i32 = 1;

/// NeXTBSD version (year, month, release)
pub const NeXTBSD: i32 = 1995064;
/// NeXTBSD 4.0
pub const NeXTBSD4_0: i32 = 0;

/// Machine-independent constants (some used in following include files).
/// Redefined constants are from POSIX 1003.1 limits file.
///
/// max command name remembered
pub const MAXCOMLEN: usize = 16;
/// max interpreter file name length
pub const MAXINTERP: usize = 64;
/// max login name length
pub const MAXLOGNAME: usize = 255;
/// max simultaneous processes
pub const MAXUPRC: usize = CHILD_MAX;
/// max bytes for an exec function
pub const NCARGS: usize = ARG_MAX;
/// max number groups
pub const NGROUPS: usize = NGROUPS_MAX;
/// default max open files per process
pub const NOFILE: usize = 256;
/// marker for empty group set member
pub const NOGROUP: gid_t = 65535;
/// max hostname size
pub const MAXHOSTNAMELEN: usize = 256;
/// maximum domain name length
pub const MAXDOMNAMELEN: usize = 256;

/// Priorities.  Note that with 32 run queues, differences less than 4 are
/// insignificant.
pub const PSWP: i32 = 0;
pub const PVM: i32 = 4;
pub const PINOD: i32 = 8;
pub const PRIBIO: i32 = 16;
pub const PVFS: i32 = 20;
// No longer magic, shouldn't be here.
pub const PZERO: i32 = 22;
pub const PSOCK: i32 = 24;
pub const PWAIT: i32 = 32;
pub const PLOCK: i32 = 36;
pub const PPAUSE: i32 = 40;
pub const PUSER: i32 = 50;
/// Priorities range from 0 through MAXPRI.
pub const MAXPRI: i32 = 127;

pub const PRIMASK: i32 = 0x0ff;
/// OR'd with pri for tsleep to check signals
pub const PCATCH: i32 = 0x100;
/// for tty SIGTTOU and SIGTTIN blocking
pub const PTTYBLOCK: i32 = 0x200;
/// OR'd with pri to stop re-aquistion of mutex upon wakeup
pub const PDROP: i32 = 0x400;
/// OR'd with pri to require mutex in spin mode upon wakeup
pub const PSPIN: i32 = 0x800;

/// number of bytes per word (integer)
pub const NBPW: usize = size_of::<i32>();

/// default file mask: S_IWGRP|S_IWOTH
pub const CMASK: i32 = 022;
/// non-existent device
pub const NODEV: dev_t = -1;

/// File system parameters and macros.
///
/// The file system is made out of blocks of at most MAXPHYS units, with
/// smaller units (fragments) only in the last direct block.  MAXBSIZE
/// primarily determines the size of buffers in the buffer pool.  It may be
/// made larger than MAXPHYS without any effect on existing file systems;
/// however making it smaller may make some file systems unmountable.
/// We set this to track the value of MAX_UPL_TRANSFER_BYTES from
/// osfmk/mach/memory_object_types.h to bound it at the maximum UPL size.
pub const MAXBSIZE: usize = 256 * 4096;
pub const MAXPHYSIO: usize = MAXPHYS;
pub const MAXFRAG: usize = 8;

pub const MAXPHYSIO_WIRED: usize = 16 * 1024 * 1024;

/// MAXPATHLEN defines the longest permissable path length after expanding
/// symbolic links. It is used to allocate a temporary buffer from the buffer
/// pool in which to do the name expansion, hence should be a power of two,
/// and must be less than or equal to MAXBSIZE.  MAXSYMLINKS defines the
/// maximum number of symbolic links that may be expanded in a path name.
/// It should be set high enough to allow all legitimate uses, but halt
/// infinite loops reasonably quickly.
pub const MAXPATHLEN: usize = PATH_MAX;
pub const MAXSYMLINKS: usize = 32;

/// Scale factor for scaled integers used to count %cpu time and load avgs.
/// The number of CPU `tick's that map to a unique `%age' can be expressed
/// by the formula (1 / (2 ^ (FSHIFT - 11))).  The maximum load average that
/// can be calculated (assuming 32 bits) can be closely approximated using
/// the formula (2 ^ (2 * (16 - FSHIFT))) for (FSHIFT < 15).
///
/// For the scheduler to maintain a 1:1 mapping of CPU `tick' to `%age',
/// FSHIFT must be at least 11; this gives us a maximum load avg of ~1024.
///
/// bits to right of fixed binary point
pub const FSHIFT: i32 = 11;
pub const FSCALE: i32 = 1 << FSHIFT;
