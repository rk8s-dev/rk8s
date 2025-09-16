// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/personality.h`

/// Flags for bug emulation.
///
/// These occupy the top three bytes.
pub const UNAME26: i32 = 0x002_0000;
/// disable randomization of VA space
pub const ADDR_NO_RANDOMIZE: i32 = 0x004_0000;
/// userspace function ptrs point to descriptors (signal handling)
pub const FDPIC_FUNCPTRS: i32 = 0x008_0000;
pub const MMAP_PAGE_ZERO: i32 = 0x010_0000;
pub const ADDR_COMPAT_LAYOUT: i32 = 0x020_0000;
pub const READ_IMPLIES_EXEC: i32 = 0x040_0000;
pub const ADDR_LIMIT_32BIT: i32 = 0x080_0000;
pub const SHORT_INODE: i32 = 0x100_0000;
pub const WHOLE_SECONDS: i32 = 0x200_0000;
pub const STICKY_TIMEOUTS: i32 = 0x400_0000;
pub const ADDR_LIMIT_3GB: i32 = 0x800_0000;

/// Security-relevant compatibility flags that must be
/// cleared upon setuid or setgid exec:
pub const PER_CLEAR_ON_SETID: i32 =
    READ_IMPLIES_EXEC | ADDR_NO_RANDOMIZE | ADDR_COMPAT_LAYOUT | MMAP_PAGE_ZERO;

/// Personality types.
///
/// These go in the low byte.  Avoid using the top bit, it will
/// conflict with error returns.
pub const PER_LINUX: i32 = 0x0000;
pub const PER_LINUX_32BIT: i32 = ADDR_LIMIT_32BIT;
pub const PER_LINUX_FDPIC: i32 = FDPIC_FUNCPTRS;
pub const PER_SVR4: i32 = 0x0001 | STICKY_TIMEOUTS | MMAP_PAGE_ZERO;
pub const PER_SVR3: i32 = 0x0002 | STICKY_TIMEOUTS | SHORT_INODE;
pub const PER_SCOSVR3: i32 = 0x0003 | STICKY_TIMEOUTS | WHOLE_SECONDS | SHORT_INODE;
pub const PER_OSR5: i32 = 0x0003 | STICKY_TIMEOUTS | WHOLE_SECONDS;
pub const PER_WYSEV386: i32 = 0x0004 | STICKY_TIMEOUTS | SHORT_INODE;
pub const PER_ISCR4: i32 = 0x0005 | STICKY_TIMEOUTS;
pub const PER_BSD: i32 = 0x0006;
pub const PER_SUNOS: i32 = 0x0006 | STICKY_TIMEOUTS;
pub const PER_XENIX: i32 = 0x0007 | STICKY_TIMEOUTS | SHORT_INODE;
pub const PER_LINUX32: i32 = 0x0008;
pub const PER_LINUX32_3GB: i32 = 0x0008 | ADDR_LIMIT_3GB;
/// IRIX5 32-bit
pub const PER_IRIX32: i32 = 0x0009 | STICKY_TIMEOUTS;
/// IRIX6 new 32-bit
pub const PER_IRIXN32: i32 = 0x000a | STICKY_TIMEOUTS;
/// IRIX6 64-bit
pub const PER_IRIX64: i32 = 0x000b | STICKY_TIMEOUTS;
pub const PER_RISCOS: i32 = 0x000c;
pub const PER_SOLARIS: i32 = 0x000d | STICKY_TIMEOUTS;
pub const PER_UW7: i32 = 0x000e | STICKY_TIMEOUTS | MMAP_PAGE_ZERO;
/// OSF/1 v4
pub const PER_OSF4: i32 = 0x000f;
pub const PER_HPUX: i32 = 0x0010;
pub const PER_MASK: i32 = 0x00ff;
