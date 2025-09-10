// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

// BEGIN of posix_types.h
pub type ino_t = usize;
pub type mode_t = u32;
pub type pid_t = i32;
pub type ipc_pid_t = i32;
pub type uid_t = u32;
pub type gid_t = u32;
pub type suseconds_t = isize;
pub type daddr_t = i32;
pub type uid32_t = u32;
pub type gid32_t = u32;

/// Most 32 bit architectures use `unsigned int` `size_t`,
/// and all 64 bit architectures use `unsigned long` `size_t`.
pub type size_t = usize;
pub type ssize_t = isize;
pub type ptrdiff_t = isize;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct fsid_t {
    pub val: [i32; 2],
}

/// Anything below here should be completely generic.
pub type off_t = isize;
pub type loff_t = i64;
pub type time_t = isize;
pub type clock_t = isize;
pub type timer_t = i32;
pub type clockid_t = i32;
pub type uid16_t = u16;
pub type gid16_t = u16;
// END of posix_types.h

// BEGIN of uapi/posix_types.h
/// This allows for 1024 file descriptors:
///
/// if `NR_OPEN` is ever grown
/// beyond that you'll have to change this too. But 1024 fd's seem to be
/// enough even for such "real" unices like OSF/1, so hopefully this is
/// one limit that doesn't have to be changed again.
///
/// Note that POSIX wants the `FD_CLEAR(fd,fdsetp)` defines to be in
/// <sys/time.h> (and thus <linux/time.h>) - but this is a more logical
/// place for them. Solved by having dummy defines in <sys/time.h>.
///
/// This macro may have been defined in <gnu/types.h>. But we always
/// use the one here.
pub const FD_SETSIZE: usize = 1024;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct fd_set_t {
    #[cfg(target_pointer_width = "64")]
    pub fds_bits: [usize; FD_SETSIZE / (8 * 64)],
    #[cfg(target_pointer_width = "32")]
    pub fds_bits: [usize; FD_SETSIZE / (8 * 32)],
}

/// Type of a SYSV IPC key.
pub type key_t = i32;
pub type mqd_t = i32;
// END of uapi/posix_types.h

// BEGIN of uapi/linux/types.h
// TODO(Shaohua): Fix endian issue
pub type be16_t = u16;
pub type le16_t = u16;
pub type be32_t = u32;
pub type le32_t = u32;
pub type be64_t = u64;
pub type le64_t = u64;

pub type poll_t = u32;
// END of uapi/linux/types.h

// BEGIN of linux/types.h
pub type dev_t = u32;
pub type umode_t = u16;
pub type nlink_t = u32;

/// The type used for indexing onto a disc or disc partition.
///
/// Linux always considers sectors to be 512 bytes long independently
/// of the devices real block size.
///
/// `blkcnt_t` is the type of the inode's block count.
pub type sector_t = u64;
pub type blkcnt_t = u64;

/// The type of an index into the pagecache.
pub type pgoff_t = usize;

pub type gfp_t = u32;
pub type slab_flags_t = u32;
pub type fmode_t = u32;

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct ustat_t {
    pub f_tfree: usize,
    pub f_tinode: ino_t,
    pub f_fname: [u8; 6],
    pub f_fpack: [u8; 6],
}
// END of linux/types.h

// BEGIN of common types
pub type uintptr_t = usize;
pub type intptr_t = usize;
pub type regoff_t = usize;
pub type register_t = usize;

pub type blksize_t = isize;
pub type fsblkcnt_t = u64;
pub type fsfilcnt_t = u64;

pub type wint_t = i32;
pub type wctype_t = usize;

pub type useconds_t = u32;

//TYPEDEF struct __pthread * pthread_t;
pub type pthread_once_t = i32;
pub type pthread_key_t = i32;
pub type pthread_spinlock_t = i32;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct pthread_mutexattr_t {
    pub attr: u32,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct pthread_condattr_t {
    pub attr: u32,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct pthread_barrierattr_t {
    pub attr: i32,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct pthread_rwlockattr_t {
    pub attr: [u32; 2],
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct io_file_t {
    pub x: u8,
}

pub type file_t = io_file_t;

// TODO(Shaohua):
//TYPEDEF struct __mbstate_t { unsigned __opaque1, __opaque2; } mbstate_t;
//TYPEDEF struct __locale_struct * locale_t;

pub type socklen_t = u32;
pub type ino64_t = u64;
// END of common types
