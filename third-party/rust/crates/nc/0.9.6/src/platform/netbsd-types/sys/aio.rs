// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Aache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/aio.h`

use crate::{off_t, sigevent_t, size_t, ssize_t};

/// Returned by aio_cancel()
pub const AIO_CANCELED: i32 = 0x1;
pub const AIO_NOTCANCELED: i32 = 0x2;
pub const AIO_ALLDONE: i32 = 0x3;

/// LIO opcodes
pub const LIO_NOP: i32 = 0x0;
pub const LIO_WRITE: i32 = 0x1;
pub const LIO_READ: i32 = 0x2;

/// LIO modes
pub const LIO_NOWAIT: i32 = 0x0;
pub const LIO_WAIT: i32 = 0x1;

/// Asynchronous I/O structure.
/// Defined in the Base Definitions volume of IEEE Std 1003.1-2001 .
#[repr(C)]
#[derive(Debug, Clone)]
pub struct aiocb_t {
    /// File offset
    pub aio_offset: off_t,

    /// I/O buffer in process space
    pub aio_buf: usize,

    /// Length of transfer
    pub aio_nbytes: size_t,

    /// File descriptor
    pub io_fildes: i32,

    /// LIO opcode
    pub aio_lio_opcode: i32,

    /// Request priority offset
    pub aio_reqprio: i32,

    /// Signal to deliver
    pub aio_sigevent: sigevent_t,

    /// Internal kernel variables
    ///
    /// State of the job
    _state: i32,

    /// Error value
    _errno: i32,

    /// Return value
    _retval: ssize_t,
}

/// Default limits of allowed AIO operations
pub const AIO_LISTIO_MAX: i32 = 512;
pub const AIO_MAX: i32 = AIO_LISTIO_MAX * 16;

/// Operations (as flags)
pub const AIO_LIO: i32 = 0x00;
pub const AIO_READ: i32 = 0x01;
pub const AIO_WRITE: i32 = 0x02;
pub const AIO_SYNC: i32 = 0x04;
pub const AIO_DSYNC: i32 = 0x08;
