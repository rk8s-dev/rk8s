// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/aio.h`

use crate::{off_t, sigevent_t, size_t, uintptr_t};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct aiocb_t {
    /// File descriptor
    pub aio_fildes: i32,

    /// File offset
    pub aio_offset: off_t,

    /// Location of buffer
    pub aio_buf: uintptr_t,

    /// Length of transfer
    pub aio_nbytes: size_t,

    /// Request priority offset
    pub aio_reqprio: i32,

    /// Signal number and value
    pub aio_sigevent: sigevent_t,

    /// Operation to be performed
    pub aio_lio_opcode: i32,
}

/// aio_cancel() return values
///
/// none of the requested operations could be canceled since they are
/// already complete.
pub const AIO_ALLDONE: i32 = 0x1;

/// all requested operations have been canceled
pub const AIO_CANCELED: i32 = 0x2;

/// some of the requested operations could not be canceled since
/// they are in progress
pub const AIO_NOTCANCELED: i32 = 0x4;

/// lio_listio operation options
///
/// option indicating that no transfer is requested
pub const LIO_NOP: i32 = 0x0;
/// option requesting a read
pub const LIO_READ: i32 = 0x1;
/// option requesting a write
pub const LIO_WRITE: i32 = 0x2;

/// lio_listio() modes
///
/// A lio_listio() synchronization operation indicating
/// that the calling thread is to continue execution while
/// the lio_listio() operation is being performed, and no
/// notification is given when the operation is complete
pub const LIO_NOWAIT: i32 = 0x1;

/// A lio_listio() synchronization operation indicating
/// that the calling thread is to suspend until the
/// lio_listio() operation is complete.
pub const LIO_WAIT: i32 = 0x2;

/// Maximum number of operations in single lio_listio call
pub const AIO_LISTIO_MAX: usize = 16;
