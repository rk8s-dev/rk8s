// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types.h`

/// Basic types upon which most other types are built.
///
/// Note: It would be nice to simply use the compiler-provided `__FOO_TYPE__`
/// macros. However, in order to do so we have to check that those match the
/// previous typedefs exactly (not just that they have the same size) since any
/// change would be an ABI break. For example, changing `long` to `long long`
/// results in different C++ name mangling.
pub type int_least8_t = i8;
pub type int_least16_t = i16;
pub type int_least32_t = i32;
pub type int_least64_t = i64;
pub type intmax_t = i64;
pub type uint_least8_t = u8;
pub type uint_least16_t = u16;
pub type uint_least32_t = u32;
pub type uint_least64_t = u64;
pub type uintmax_t = u64;

pub type intptr_t = isize;
pub type intfptr = isize;
pub type uintptr = usize;
pub type vm_offset_t = usize;
pub type vm_size_t = usize;

/// `sizeof()`
pub type size_t = usize;

/// byte count or error
pub type ssize_t = isize;

/// ptr1 - ptr2
pub type ptrdiff_t = isize;

// Target-dependent type definitions are imported automatically.

/// Standard type definitions.
/// file block size.
pub type blksize_t = i32;

/// file block count.
pub type blkcnt_t = i64;

/// `clock_gettime()`...
pub type clockid_t = i32;

/// file flags.
pub type fflags_t = u32;

pub type fsblkcnt_t = u64;
pub type fsfilcnt_t = u64;
pub type gid_t = u32;

/// can hold a `gid_t`, `pid_t`, or `uid_t`.
pub type id_t = i64;

/// inode number.
pub type ino_t = u64;

/// IPC key (for Sys V IPC).
pub type key_t = isize;

/// Thread ID (a.k.a. LWP).
pub type lwpid_t = i32;

/// permissions.
pub type mode_t = u16;

/// access permissions.
pub type accmode_t = i32;
pub type nl_item_t = i32;

/// link count.
pub type nlink_t = u64;

/// file offset.
pub type off_t = i64;

/// file offset (alias).
pub type off64_t = i64;

/// process or process group.
pub type pid_t = i32;

/// resource limit - intentionally.
///
/// signed, because of legacy code that uses -1 for `RLIM_INFINITY`
pub type rlim_t = i64;

pub type sa_family_t = u8;
pub type socklen_t = u32;

/// microseconds (signed).
pub type suseconds_t = isize;
//typedef	struct __timer	*__timer_t;	/* timer_gettime()... */
//typedef	struct __mq	*__mqd_t;	/* mq_open()... */
pub type uid_t = u32;

/// microseconds (unsigned).
pub type useconds_t = u32;

/// which parameter for cpuset.
pub type cpuwhich_t = i32;

/// level parameter for cpuset.
pub type cpulevel_t = i32;

/// cpuset identifier.
pub type cpusetid_t = i32;

/// bwrite(3), FIOBMAP2, etc.
pub type daddr_t = i64;

// Unusual type definitions.
//
// rune_t is declared to be an ``int'' instead of the more natural
// `unsigned long` or `long`.  Two things are happening here.  It is not
// unsigned so that EOF (-1) can be naturally assigned to it and used.  Also,
// it looks like 10646 will be a 31 bit standard.  This means that if your
// ints cannot hold 32 bits, you will be in trouble.  The reason an int was
// chosen over a long is that the is*() and to*() routines take ints (says
// ANSI C), but they use __ct_rune_t instead of int.
//
/// arg type for ctype funcs.
pub type ct_rune_t = i32;

/// `rune_t` (see above)
pub type rune_t = ct_rune_t;

/// `wint_t` (see above).
pub type wint_t = ct_rune_t;

/// device number.
pub type dev_t = u64;

/// fixed point number.
pub type fixpt_t = u32;

/// `mbstate_t` is an opaque object to keep conversion state during multibyte
/// stream conversions.
#[repr(C)]
pub union mbstate_t {
    pub mbstate8: [u8; 128],

    /// for alignment
    pub mbstateL: i64,
}

pub type rman_res_t = uintmax_t;

/// When the following macro is defined, the system uses 64-bit inode numbers.
/// Programs can use this to avoid including <sys/param.h>, with its associated
/// namespace pollution.
pub const __INO64: bool = true;
