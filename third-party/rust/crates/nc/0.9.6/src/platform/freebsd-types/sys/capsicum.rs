// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/capsicum.h`
//!
//! Possible rights on capabilities.
//!
//! Notes:
//! Some system calls don't require a capability in order to perform an
//! operation on an fd.  These include: close, dup, dup2.
//!
//! sendfile is authorized using CAP_READ on the file and CAP_WRITE on the
//! socket.
//!
//! mmap() and aio*() system calls will need special attention as they may
//! involve reads or writes depending a great deal on context.

use crate::{cap_rights_t, CAP_RIGHTS_VERSION, F_GETFL, F_GETOWN, F_SETFL, F_SETOWN, SSIZE_MAX};

pub const fn CAPRIGHT(idx: usize, bit: u64) -> u64 {
    (1_u64 << (57 + idx)) | bit
}

// INDEX 0
/// General file I/O.
/// Allows for openat(O_RDONLY), read(2), readv(2).
pub const CAP_READ: u64 = CAPRIGHT(0, 0x0000000000000001);
/// Allows for openat(O_WRONLY | O_APPEND), write(2), writev(2).
pub const CAP_WRITE: u64 = CAPRIGHT(0, 0x0000000000000002);
/// Allows for lseek(fd, 0, SEEK_CUR).
pub const CAP_SEEK_TELL: u64 = CAPRIGHT(0, 0x0000000000000004);
/// Allows for lseek(2).
pub const CAP_SEEK: u64 = CAP_SEEK_TELL | 0x0000000000000008;
/// Allows for aio_read(2), pread(2), preadv(2).
pub const CAP_PREAD: u64 = CAP_SEEK | CAP_READ;
/// Allows for aio_write(2), openat(O_WRONLY) (without O_APPEND), pwrite(2), pwritev(2).
pub const CAP_PWRITE: u64 = CAP_SEEK | CAP_WRITE;
/// Allows for mmap(PROT_NONE).
pub const CAP_MMAP: u64 = CAPRIGHT(0, 0x0000000000000010);
/// Allows for mmap(PROT_READ).
pub const CAP_MMAP_R: u64 = CAP_MMAP | CAP_SEEK | CAP_READ;
/// Allows for mmap(PROT_WRITE).
pub const CAP_MMAP_W: u64 = CAP_MMAP | CAP_SEEK | CAP_WRITE;
/// Allows for mmap(PROT_EXEC).
pub const CAP_MMAP_X: u64 = CAP_MMAP | CAP_SEEK | 0x0000000000000020;
/// Allows for mmap(PROT_READ | PROT_WRITE).
pub const CAP_MMAP_RW: u64 = CAP_MMAP_R | CAP_MMAP_W;
/// Allows for mmap(PROT_READ | PROT_EXEC).
pub const CAP_MMAP_RX: u64 = CAP_MMAP_R | CAP_MMAP_X;
/// Allows for mmap(PROT_WRITE | PROT_EXEC).
pub const CAP_MMAP_WX: u64 = CAP_MMAP_W | CAP_MMAP_X;
/// Allows for mmap(PROT_READ | PROT_WRITE | PROT_EXEC).
pub const CAP_MMAP_RWX: u64 = CAP_MMAP_R | CAP_MMAP_W | CAP_MMAP_X;
/// Allows for openat(O_CREAT).
pub const CAP_CREATE: u64 = CAPRIGHT(0, 0x0000000000000040);
/// Allows for openat(O_EXEC) and fexecve(2) in turn.
pub const CAP_FEXECVE: u64 = CAPRIGHT(0, 0x0000000000000080);
/// Allows for openat(O_SYNC), openat(O_FSYNC), fsync(2), aio_fsync(2).
pub const CAP_FSYNC: u64 = CAPRIGHT(0, 0x0000000000000100);
/// Allows for openat(O_TRUNC), ftruncate(2).
pub const CAP_FTRUNCATE: u64 = CAPRIGHT(0, 0x0000000000000200);

/// Lookups - used to constrain *at() calls.
pub const CAP_LOOKUP: u64 = CAPRIGHT(0, 0x0000000000000400);

/// VFS methods.
/// Allows for fchdir(2).
pub const CAP_FCHDIR: u64 = CAPRIGHT(0, 0x0000000000000800);
/// Allows for fchflags(2).
pub const CAP_FCHFLAGS: u64 = CAPRIGHT(0, 0x0000000000001000);
/// Allows for fchflags(2) and chflagsat(2).
pub const CAP_CHFLAGSAT: u64 = CAP_FCHFLAGS | CAP_LOOKUP;
/// Allows for fchmod(2).
pub const CAP_FCHMOD: u64 = CAPRIGHT(0, 0x0000000000002000);
/// Allows for fchmod(2) and fchmodat(2).
pub const CAP_FCHMODAT: u64 = CAP_FCHMOD | CAP_LOOKUP;
/// Allows for fchown(2).
pub const CAP_FCHOWN: u64 = CAPRIGHT(0, 0x0000000000004000);
/// Allows for fchown(2) and fchownat(2).
pub const CAP_FCHOWNAT: u64 = CAP_FCHOWN | CAP_LOOKUP;
/// Allows for fcntl(2).
pub const CAP_FCNTL: u64 = CAPRIGHT(0, 0x0000000000008000);
/// Allows for flock(2), openat(O_SHLOCK), openat(O_EXLOCK),
/// fcntl(F_SETLK_REMOTE), fcntl(F_SETLKW), fcntl(F_SETLK), fcntl(F_GETLK).
pub const CAP_FLOCK: u64 = CAPRIGHT(0, 0x0000000000010000);
/// Allows for fpathconf(2).
pub const CAP_FPATHCONF: u64 = CAPRIGHT(0, 0x0000000000020000);
/// Allows for UFS background-fsck operations.
pub const CAP_FSCK: u64 = CAPRIGHT(0, 0x0000000000040000);
/// Allows for fstat(2).
pub const CAP_FSTAT: u64 = CAPRIGHT(0, 0x0000000000080000);
/// Allows for fstat(2), fstatat(2) and faccessat(2).
pub const CAP_FSTATAT: u64 = CAP_FSTAT | CAP_LOOKUP;
/// Allows for fstatfs(2).
pub const CAP_FSTATFS: u64 = CAPRIGHT(0, 0x0000000000100000);
/// Allows for futimens(2) and futimes(2).
pub const CAP_FUTIMES: u64 = CAPRIGHT(0, 0x0000000000200000);
/// Allows for futimens(2), futimes(2), futimesat(2) and utimensat(2).
pub const CAP_FUTIMESAT: u64 = CAP_FUTIMES | CAP_LOOKUP;
/// Allows for linkat(2) (target directory descriptor).
pub const CAP_LINKAT_TARGET: u64 = CAP_LOOKUP | 0x0000000000400000;
/// Allows for mkdirat(2).
pub const CAP_MKDIRAT: u64 = CAP_LOOKUP | 0x0000000000800000;
/// Allows for mkfifoat(2).
pub const CAP_MKFIFOAT: u64 = CAP_LOOKUP | 0x0000000001000000;
/// Allows for mknodat(2).
pub const CAP_MKNODAT: u64 = CAP_LOOKUP | 0x0000000002000000;
/// Allows for renameat(2) (source directory descriptor).
pub const CAP_RENAMEAT_SOURCE: u64 = CAP_LOOKUP | 0x0000000004000000;
/// Allows for symlinkat(2).
pub const CAP_SYMLINKAT: u64 = CAP_LOOKUP | 0x0000000008000000;
/// Allows for unlinkat(2) and renameat(2) if destination object exists and will be removed.
pub const CAP_UNLINKAT: u64 = CAP_LOOKUP | 0x0000000010000000;

/// Socket operations.
/// Allows for accept(2) and accept4(2).
pub const CAP_ACCEPT: u64 = CAPRIGHT(0, 0x0000000020000000);
/// Allows for bind(2).
pub const CAP_BIND: u64 = CAPRIGHT(0, 0x0000000040000000);
/// Allows for connect(2).
pub const CAP_CONNECT: u64 = CAPRIGHT(0, 0x0000000080000000);
/// Allows for getpeername(2).
pub const CAP_GETPEERNAME: u64 = CAPRIGHT(0, 0x0000000100000000);
/// Allows for getsockname(2).
pub const CAP_GETSOCKNAME: u64 = CAPRIGHT(0, 0x0000000200000000);
/// Allows for getsockopt(2).
pub const CAP_GETSOCKOPT: u64 = CAPRIGHT(0, 0x0000000400000000);
/// Allows for listen(2).
pub const CAP_LISTEN: u64 = CAPRIGHT(0, 0x0000000800000000);
/// Allows for sctp_peeloff(2).
pub const CAP_PEELOFF: u64 = CAPRIGHT(0, 0x0000001000000000);
pub const CAP_RECV: u64 = CAP_READ;
pub const CAP_SEND: u64 = CAP_WRITE;
/// Allows for setsockopt(2).
pub const CAP_SETSOCKOPT: u64 = CAPRIGHT(0, 0x0000002000000000);
/// Allows for shutdown(2).
pub const CAP_SHUTDOWN: u64 = CAPRIGHT(0, 0x0000004000000000);

/// Allows for bindat(2) on a directory descriptor.
pub const CAP_BINDAT: u64 = CAP_LOOKUP | 0x0000008000000000;
/// Allows for connectat(2) on a directory descriptor.
pub const CAP_CONNECTAT: u64 = CAP_LOOKUP | 0x0000010000000000;

/// Allows for linkat(2) (source directory descriptor).
pub const CAP_LINKAT_SOURCE: u64 = CAP_LOOKUP | 0x0000020000000000;
/// Allows for renameat(2) (target directory descriptor).
pub const CAP_RENAMEAT_TARGET: u64 = CAP_LOOKUP | 0x0000040000000000;

pub const CAP_SOCK_CLIENT: u64 = CAP_CONNECT
    | CAP_GETPEERNAME
    | CAP_GETSOCKNAME
    | CAP_GETSOCKOPT
    | CAP_PEELOFF
    | CAP_RECV
    | CAP_SEND
    | CAP_SETSOCKOPT
    | CAP_SHUTDOWN;
pub const CAP_SOCK_SERVER: u64 = CAP_ACCEPT
    | CAP_BIND
    | CAP_GETPEERNAME
    | CAP_GETSOCKNAME
    | CAP_GETSOCKOPT
    | CAP_LISTEN
    | CAP_PEELOFF
    | CAP_RECV
    | CAP_SEND
    | CAP_SETSOCKOPT
    | CAP_SHUTDOWN;

/// All used bits for index 0.
pub const CAP_ALL0: u64 = CAPRIGHT(0, 0x000007FFFFFFFFFF);

/// Available bits for index 0.
pub const CAP_UNUSED0_44: u64 = CAPRIGHT(0, 0x0000080000000000);
/// ...
pub const CAP_UNUSED0_57: u64 = CAPRIGHT(0, 0x0100000000000000);

// INDEX 1
/// Mandatory Access Control.
/// Allows for mac_get_fd(3).
pub const CAP_MAC_GET: u64 = CAPRIGHT(1, 0x0000000000000001);
/// Allows for mac_set_fd(3).
pub const CAP_MAC_SET: u64 = CAPRIGHT(1, 0x0000000000000002);

/// Methods on semaphores.
pub const CAP_SEM_GETVALUE: u64 = CAPRIGHT(1, 0x0000000000000004);
pub const CAP_SEM_POST: u64 = CAPRIGHT(1, 0x0000000000000008);
pub const CAP_SEM_WAIT: u64 = CAPRIGHT(1, 0x0000000000000010);

/// Allows select(2) and poll(2) on descriptor.
pub const CAP_EVENT: u64 = CAPRIGHT(1, 0x0000000000000020);
/// Allows for kevent(2) on kqueue descriptor with eventlist != NULL.
pub const CAP_KQUEUE_EVENT: u64 = CAPRIGHT(1, 0x0000000000000040);

/// Strange and powerful rights that should not be given lightly.
/// Allows for ioctl(2).
pub const CAP_IOCTL: u64 = CAPRIGHT(1, 0x0000000000000080);
pub const CAP_TTYHOOK: u64 = CAPRIGHT(1, 0x0000000000000100);

/// Process management via process descriptors.
/// Allows for pdgetpid(2).
pub const CAP_PDGETPID: u64 = CAPRIGHT(1, 0x0000000000000200);
/// Allows for pdwait4(2).
pub const CAP_PDWAIT: u64 = CAPRIGHT(1, 0x0000000000000400);
/// Allows for pdkill(2).
pub const CAP_PDKILL: u64 = CAPRIGHT(1, 0x0000000000000800);

/// Extended attributes.
/// Allows for extattr_delete_fd(2).
pub const CAP_EXTATTR_DELETE: u64 = CAPRIGHT(1, 0x0000000000001000);
/// Allows for extattr_get_fd(2).
pub const CAP_EXTATTR_GET: u64 = CAPRIGHT(1, 0x0000000000002000);
/// Allows for extattr_list_fd(2).
pub const CAP_EXTATTR_LIST: u64 = CAPRIGHT(1, 0x0000000000004000);
/// Allows for extattr_set_fd(2).
pub const CAP_EXTATTR_SET: u64 = CAPRIGHT(1, 0x0000000000008000);

/// Access Control Lists.
/// Allows for acl_valid_fd_np(3).
pub const CAP_ACL_CHECK: u64 = CAPRIGHT(1, 0x0000000000010000);
/// Allows for acl_delete_fd_np(3).
pub const CAP_ACL_DELETE: u64 = CAPRIGHT(1, 0x0000000000020000);
/// Allows for acl_get_fd(3) and acl_get_fd_np(3).
pub const CAP_ACL_GET: u64 = CAPRIGHT(1, 0x0000000000040000);
/// Allows for acl_set_fd(3) and acl_set_fd_np(3).
pub const CAP_ACL_SET: u64 = CAPRIGHT(1, 0x0000000000080000);

/// Allows for kevent(2) on kqueue descriptor with changelist != NULL.
pub const CAP_KQUEUE_CHANGE: u64 = CAPRIGHT(1, 0x0000000000100000);

pub const CAP_KQUEUE: u64 = CAP_KQUEUE_EVENT | CAP_KQUEUE_CHANGE;

/// All used bits for index 1.
pub const CAP_ALL1: u64 = CAPRIGHT(1, 0x00000000001FFFFF);

/// Available bits for index 1.
pub const CAP_UNUSED1_22: u64 = CAPRIGHT(1, 0x0000000000200000);
/// ...
pub const CAP_UNUSED1_57: u64 = CAPRIGHT(1, 0x0100000000000000);

/// Backward compatibility.
pub const CAP_POLL_EVENT: u64 = CAP_EVENT;

pub fn CAP_ALL(rights: &mut cap_rights_t) {
    rights.cr_rights[0] = ((CAP_RIGHTS_VERSION as u64) << 62) | CAP_ALL0;
    rights.cr_rights[1] = CAP_ALL1;
}

pub fn CAP_NONE(rights: &mut cap_rights_t) {
    rights.cr_rights[0] = ((CAP_RIGHTS_VERSION as u64) << 62) | CAPRIGHT(0, 0);
    rights.cr_rights[1] = CAPRIGHT(1, 0);
}

pub const fn CAPRVER(right: u64) -> u64 {
    right >> 62
}

pub const fn CAPVER(rights: &cap_rights_t) -> u64 {
    CAPRVER(rights.cr_rights[0])
}

pub const fn CAPARSIZE(rights: &cap_rights_t) -> u64 {
    CAPVER(rights) + 2
}

pub const fn CAPIDXBIT(right: u64) -> u64 {
    (right >> 57) & 0x1F
}

/// Allowed fcntl(2) commands.
pub const CAP_FCNTL_GETFL: u64 = 1 << F_GETFL;
pub const CAP_FCNTL_SETFL: u64 = 1 << F_SETFL;
pub const CAP_FCNTL_GETOWN: u64 = 1 << F_GETOWN;
pub const CAP_FCNTL_SETOWN: u64 = 1 << F_SETOWN;
pub const CAP_FCNTL_ALL: u64 =
    CAP_FCNTL_GETFL | CAP_FCNTL_SETFL | CAP_FCNTL_GETOWN | CAP_FCNTL_SETOWN;

pub const CAP_IOCTLS_ALL: u64 = SSIZE_MAX as u64;
