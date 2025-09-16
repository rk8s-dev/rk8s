// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/aio.h`

use crate::{off_t, sigevent_t, size_t};

/// Returned by aio_cancel:
pub const AIO_CANCELED: i32 = 0x1;
pub const AIO_NOTCANCELED: i32 = 0x2;
pub const AIO_ALLDONE: i32 = 0x3;

/// LIO opcodes
pub const LIO_NOP: i32 = 0x0;
pub const LIO_WRITE: i32 = 0x1;
pub const LIO_READ: i32 = 0x2;
pub const LIO_VECTORED: i32 = 0x4;
pub const LIO_WRITEV: i32 = LIO_WRITE | LIO_VECTORED;
pub const LIO_READV: i32 = LIO_READ | LIO_VECTORED;
pub const LIO_SYNC: i32 = 0x8;
pub const LIO_DSYNC: i32 = 0x10 | LIO_SYNC;
pub const LIO_MLOCK: i32 = 0x20;

/// LIO modes
pub const LIO_NOWAIT: i32 = 0x0;
pub const LIO_WAIT: i32 = 0x1;

/// Maximum number of operations in a single `lio_listio` call
pub const AIO_LISTIO_MAX: i32 = 16;

/// Private members for aiocb -- don't access directly.
#[repr(C)]
struct __aiocb_private_t {
    status: isize,
    error: isize,
    kernelinfo: usize,
}

/// I/O control block
#[repr(C)]
pub struct aiocb_t {
    /// File descriptor
    pub aio_fildes: i32,

    /// File offset for I/O
    pub aio_offset: off_t,

    /// I/O buffer in process space
    //volatile void *aio_buf;
    pub aio_buf: usize,

    /// Number of bytes for I/O
    pub aio_nbytes: size_t,

    __spare__: [i32; 2],
    __spare2__: usize,

    /// LIO opcode
    pub aio_lio_opcode: i32,

    /// Request priority -- ignored
    pub aio_reqprio: i32,
    _aiocb_private: __aiocb_private_t,

    /// Signal to deliver
    pub aio_sigevent: sigevent_t,
}
