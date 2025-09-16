// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `uapi/linux/io_uring.h`

#![allow(clippy::module_name_repetitions)]

use core::fmt;

use crate::off_t;
use crate::rwf_t;

#[repr(C)]
#[derive(Clone, Copy)]
pub union io_uring_sqe_file_off_t {
    /// offset into file
    pub off: u64,

    pub addr2: u64,
}

impl fmt::Debug for io_uring_sqe_file_off_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "off: {}", unsafe { self.off })
    }
}

impl Default for io_uring_sqe_file_off_t {
    fn default() -> Self {
        Self { off: 0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union io_uring_sqe_buf_addr_t {
    /// pointer to buffer or iovecs
    pub addr: u64,

    pub splice_off_in: u64,
}

impl fmt::Debug for io_uring_sqe_buf_addr_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "addr: {}", unsafe { self.addr })
    }
}

impl Default for io_uring_sqe_buf_addr_t {
    fn default() -> Self {
        Self { addr: 0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union io_uring_sqe_other_flags_t {
    pub rw_flags: rwf_t,
    pub fsync_flags: u32,
    pub poll_events: u16,
    pub sync_range_flags: u32,
    pub msg_flags: u32,
    pub timeout_flags: u32,
    pub accept_flags: u32,
    pub cancel_flags: u32,
    pub open_flags: u32,
    pub statx_flags: u32,
    pub fadvise_advice: u32,
    pub splice_flags: u32,
}

impl fmt::Debug for io_uring_sqe_other_flags_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rw_flags: {}", unsafe { self.rw_flags })
    }
}

impl Default for io_uring_sqe_other_flags_t {
    fn default() -> Self {
        Self { rw_flags: 0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union io_uring_sqe_buf_group_t {
    /// index into fixed buffers, if used
    pub buf_index: u16,

    /// for grouped buffer selection
    pub buf_group: u16,
}

impl fmt::Debug for io_uring_sqe_buf_group_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "buf_index: {}", unsafe { self.buf_index })
    }
}

impl Default for io_uring_sqe_buf_group_t {
    fn default() -> Self {
        Self { buf_index: 0 }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct io_uring_sqe_buf_t {
    /// pack this to avoid bogus arm OABI complaints
    pub group: io_uring_sqe_buf_group_t,

    /// personality to use, if used
    pub personality: u16,
    pub splice_fd_in: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union io_uring_sqe_opt_buf_t {
    pub buf: io_uring_sqe_buf_t,
    pad2: [u64; 3],
}

impl fmt::Debug for io_uring_sqe_opt_buf_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", unsafe { self.buf })
    }
}

impl Default for io_uring_sqe_opt_buf_t {
    fn default() -> Self {
        Self {
            buf: io_uring_sqe_buf_t::default(),
        }
    }
}

/// IO submission data structure (Submission Queue Entry)
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct io_uring_sqe_t {
    /// type of operation for this sqe
    pub opcode: IOURING_OP,

    /// IOSQE_ flags
    pub flags: u8,

    /// ioprio for the request
    pub ioprio: u16,

    /// file descriptor to do IO on
    pub fd: i32,

    pub file_off: io_uring_sqe_file_off_t,

    pub buf_addr: io_uring_sqe_buf_addr_t,

    /// buffer size or number of iovecs
    pub len: u32,

    pub other_flags: io_uring_sqe_other_flags_t,

    /// data to be passed back at completion time
    pub user_data: u64,

    pub opt_buf: io_uring_sqe_opt_buf_t,
}

pub const IOSQE_FIXED_FILE_BIT: u32 = 0;
pub const IOSQE_IO_DRAIN_BIT: u32 = 1;
pub const IOSQE_IO_LINK_BIT: u32 = 2;
pub const IOSQE_IO_HARDLINK_BIT: u32 = 3;
pub const IOSQE_ASYNC_BIT: u32 = 4;
pub const IOSQE_BUFFER_SELECT_BIT: u32 = 5;

/// `sqe->flags`
/// use fixed fileset
pub const IOSQE_FIXED_FILE: u32 = 1 << IOSQE_FIXED_FILE_BIT;
/// issue after inflight IO
pub const IOSQE_IO_DRAIN: u32 = 1 << IOSQE_IO_DRAIN_BIT;
/// links next sqe
pub const IOSQE_IO_LINK: u32 = 1 << IOSQE_IO_LINK_BIT;
/// like LINK, but stronger
pub const IOSQE_IO_HARDLINK: u32 = 1 << IOSQE_IO_HARDLINK_BIT;
/// always go async
pub const IOSQE_ASYNC: u32 = 1 << IOSQE_ASYNC_BIT;
/// select buffer from `sqe->buf_group`
pub const IOSQE_BUFFER_SELECT: u32 = 1 << IOSQE_BUFFER_SELECT_BIT;

/// `io_uring_setup()` flags
/// `io_context` is polled
pub const IORING_SETUP_IOPOLL: u32 = 1;
/// SQ poll thread
pub const IORING_SETUP_SQPOLL: u32 = 1 << 1;
/// `sq_thread_cpu` is valid
pub const IORING_SETUP_SQ_AFF: u32 = 1 << 2;
/// app defines CQ size
pub const IORING_SETUP_CQSIZE: u32 = 1 << 3;
/// clamp SQ/CQ ring sizes
pub const IORING_SETUP_CLAMP: u32 = 1 << 4;
/// attach to existing wq
pub const IORING_SETUP_ATTACH_WQ: u32 = 1 << 5;

#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
pub enum IOURING_OP {
    #[default]
    IORING_OP_NOP,
    IORING_OP_READV,
    IORING_OP_WRITEV,
    IORING_OP_FSYNC,
    IORING_OP_READ_FIXED,
    IORING_OP_WRITE_FIXED,
    IORING_OP_POLL_ADD,
    IORING_OP_POLL_REMOVE,
    IORING_OP_SYNC_FILE_RANGE,
    IORING_OP_SENDMSG,
    IORING_OP_RECVMSG,
    IORING_OP_TIMEOUT,
    IORING_OP_TIMEOUT_REMOVE,
    IORING_OP_ACCEPT,
    IORING_OP_ASYNC_CANCEL,
    IORING_OP_LINK_TIMEOUT,
    IORING_OP_CONNECT,
    IORING_OP_FALLOCATE,
    IORING_OP_OPENAT,
    IORING_OP_CLOSE,
    IORING_OP_FILES_UPDATE,
    IORING_OP_STATX,
    IORING_OP_READ,
    IORING_OP_WRITE,
    IORING_OP_FADVISE,
    IORING_OP_MADVISE,
    IORING_OP_SEND,
    IORING_OP_RECV,
    IORING_OP_OPENAT2,
    IORING_OP_EPOLL_CTL,
    IORING_OP_SPLICE,
    IORING_OP_PROVIDE_BUFFERS,
    IORING_OP_REMOVE_BUFFERS,

    /// this goes last, obviously
    IORING_OP_LAST,
}

/// `sqe->fsync_flags`
pub const IORING_FSYNC_DATASYNC: u32 = 1;

/// `sqe->timeout_flags`
pub const IORING_TIMEOUT_ABS: u32 = 1;

/// `sqe->splice_flags`
///
/// extends `splice(2)` flags
/// the last bit of u32
pub const SPLICE_F_FD_IN_FIXED: u32 = 1 << 31;

/// IO completion data structure (Completion Queue Entry)
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct io_uring_cqe_t {
    /// sqe->data submission passed back
    pub user_data: u64,

    /// result code for this event
    pub res: i32,

    pub flags: u32,
}

/// `cqe->flags`
///
/// `IORING_CQE_F_BUFFER`: If set, the upper 16 bits are the buffer ID
pub const IORING_CQE_F_BUFFER: u32 = 1;
pub const IORING_CQE_BUFFER_SHIFT: i32 = 16;

/// Magic offsets for the application to mmap the data it needs
pub const IORING_OFF_SQ_RING: off_t = 0;
pub const IORING_OFF_CQ_RING: off_t = 0x0800_0000;
pub const IORING_OFF_SQES: off_t = 0x1000_0000;

/// Filled with the offset for `mmap(2)`
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct io_sqring_offsets_t {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub flags: u32,
    pub dropped: u32,
    pub array: u32,
    pub resv1: u32,
    pub resv2: u64,
}

/// `sq_ring->flags`
/// needs `io_uring_enter` wakeup
pub const IORING_SQ_NEED_WAKEUP: u32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct io_cqring_offsets_t {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub overflow: u32,
    pub cqes: u32,
    pub resv: [u64; 2],
}

/// `io_uring_enter(2)` flags
pub const IORING_ENTER_GETEVENTS: u32 = 1;
pub const IORING_ENTER_SQ_WAKEUP: u32 = 1 << 1;

/// Passed in for `io_uring_setup(2)`. Copied back with updated info on success
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct io_uring_params_t {
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq_thread_cpu: u32,
    pub sq_thread_idle: u32,
    pub features: u32,
    pub wq_fd: u32,
    pub resv: [u32; 3],
    pub sq_off: io_sqring_offsets_t,
    pub cq_off: io_cqring_offsets_t,
}

/// `io_uring_params->features` flags
pub const IORING_FEAT_SINGLE_MMAP: u32 = 1;
pub const IORING_FEAT_NODROP: u32 = 1 << 1;
pub const IORING_FEAT_SUBMIT_STABLE: u32 = 1 << 2;
pub const IORING_FEAT_RW_CUR_POS: u32 = 1 << 3;
pub const IORING_FEAT_CUR_PERSONALITY: u32 = 1 << 4;
pub const IORING_FEAT_FAST_POLL: u32 = 1 << 5;

/// `io_uring_register(2)` opcodes and arguments
pub const IORING_REGISTER_BUFFERS: i32 = 0;
pub const IORING_UNREGISTER_BUFFERS: i32 = 1;
pub const IORING_REGISTER_FILES: i32 = 2;
pub const IORING_UNREGISTER_FILES: i32 = 3;
pub const IORING_REGISTER_EVENTFD: i32 = 4;
pub const IORING_UNREGISTER_EVENTFD: i32 = 5;
pub const IORING_REGISTER_FILES_UPDATE: i32 = 6;
pub const IORING_REGISTER_EVENTFD_ASYNC: i32 = 7;
pub const IORING_REGISTER_PROBE: i32 = 8;
pub const IORING_REGISTER_PERSONALITY: i32 = 9;
pub const IORING_UNREGISTER_PERSONALITY: i32 = 10;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct io_uring_files_update_t {
    pub offset: u32,
    pub resv: u32,
    pub fds: u64,
}

pub const IO_URING_OP_SUPPORTED: u32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct io_uring_probe_op_t {
    pub op: u8,
    pub resv: u8,

    /// `IO_URING_OP`_* flags
    pub flags: u16,

    pub resv2: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct io_uring_probe_t {
    /// last opcode supported
    pub last_op: u8,

    /// length of ops[] array below
    pub ops_len: u8,

    pub resv: u16,
    pub resv2: [u32; 3],
    pub ops: [io_uring_probe_op_t; 0],
}
