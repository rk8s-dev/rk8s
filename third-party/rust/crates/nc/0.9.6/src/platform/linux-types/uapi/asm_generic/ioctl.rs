// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/ioctl.h`
//!
//! ioctl command encoding: 32 bits total, command in lower 16 bits,
//! size of the parameter structure in the lower 14 bits of the
//! upper 16 bits.
//! Encoding the size of the parameter structure in the ioctl request
//! is useful for catching programs compiled with old versions
//! and to avoid overwriting user space outside the user buffer area.
//! The highest 2 bits are reserved for indicating the `access mode`.
//! NOTE: This limits the max parameter size to 16kB -1 !
//!
//! The following is for compatibility across the various Linux
//! platforms.  The generic ioctl numbering scheme doesn't really enforce
//! a type field.  De facto, however, the top 8 bits of the lower 16
//! bits are indeed used as a type field, so we might just as well make
//! this explicit here.  Please be sure to use the decoding macros
//! below from now on.

use core::mem::size_of;

pub const IOC_NRBITS: u32 = 8;
pub const IOC_TYPEBITS: u32 = 8;

/// Let any architecture override either of the following before including this file.
pub const IOC_SIZEBITS: u32 = 14;

pub const IOC_DIRBITS: u32 = 2;

pub const IOC_NRMASK: u32 = (1 << IOC_NRBITS) - 1;
pub const IOC_TYPEMASK: u32 = (1 << IOC_TYPEBITS) - 1;
pub const IOC_SIZEMASK: u32 = (1 << IOC_SIZEBITS) - 1;
pub const IOC_DIRMASK: u32 = (1 << IOC_DIRBITS) - 1;

pub const IOC_NRSHIFT: u32 = 0;
pub const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
pub const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
pub const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;

/// Direction bits, which any architecture can choose to override
/// before including this file.
///
/// NOTE: `_IOC_WRITE` means userland is writing and kernel is
/// reading. `_IOC_READ` means userland is reading and kernel is writing.
pub const IOC_NONE: u32 = 0;
pub const IOC_WRITE: u32 = 1;
pub const IOC_READ: u32 = 2;

#[inline]
#[must_use]
pub const fn IOC(dir: u32, type_: u8, nr: u32, size: u32) -> u32 {
    let type_ = type_ as u32;
    (dir << IOC_DIRSHIFT) | (type_ << IOC_TYPESHIFT) | (nr << IOC_NRSHIFT) | (size << IOC_SIZESHIFT)
}

#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub const fn IOC_TYPECHECK<T>() -> u32 {
    size_of::<T>() as u32
}

/// Used to create numbers.
///
/// NOTE: _IOW means userland is writing and kernel is reading. _IOR
/// means userland is reading and kernel is writing.
#[inline]
#[must_use]
pub const fn IO(type_: u8, nr: u32) -> u32 {
    IOC(IOC_NONE, type_, nr, 0)
}

#[inline]
#[must_use]
pub const fn IOR<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_READ, type_, nr, IOC_TYPECHECK::<T>())
}

#[inline]
#[must_use]
pub const fn IOW<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_WRITE, type_, nr, IOC_TYPECHECK::<T>())
}

#[inline]
#[must_use]
pub const fn IOWR<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_READ | IOC_WRITE, type_, nr, IOC_TYPECHECK::<T>())
}

#[inline]
#[must_use]
pub const fn IOR_BAD<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_READ, type_, nr, IOC_TYPECHECK::<T>())
}

#[inline]
#[must_use]
pub const fn IOW_BAD<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_WRITE, type_, nr, IOC_TYPECHECK::<T>())
}

#[inline]
#[must_use]
pub const fn IOWR_BAD<T>(type_: u8, nr: u32) -> u32 {
    IOC(IOC_READ | IOC_WRITE, type_, nr, IOC_TYPECHECK::<T>())
}

/// used to decode ioctl numbers..
#[inline]
#[must_use]
pub const fn IOC_DIR(nr: u32) -> u32 {
    (nr >> IOC_DIRSHIFT) & IOC_DIRMASK
}

#[inline]
#[must_use]
pub const fn IOC_TYPE(nr: u32) -> u32 {
    nr >> IOC_TYPESHIFT & IOC_TYPEMASK
}

#[inline]
#[must_use]
pub const fn IOC_NR(nr: u32) -> u32 {
    (nr >> IOC_NRSHIFT) & IOC_NRMASK
}

#[inline]
#[must_use]
pub const fn IOC_SIZE(nr: u32) -> u32 {
    nr >> IOC_SIZESHIFT & IOC_SIZEMASK
}

/// ...and for the drivers/sound files...
pub const IOC_IN: u32 = IOC_WRITE << IOC_DIRSHIFT;
pub const IOC_OUT: u32 = IOC_READ << IOC_DIRSHIFT;
pub const IOC_INOUT: u32 = (IOC_WRITE | IOC_READ) << IOC_DIRSHIFT;
pub const IOCSIZE_MASK: u32 = IOC_SIZEMASK << IOC_SIZESHIFT;
pub const IOCSIZE_SHIFT: u32 = IOC_SIZESHIFT;
