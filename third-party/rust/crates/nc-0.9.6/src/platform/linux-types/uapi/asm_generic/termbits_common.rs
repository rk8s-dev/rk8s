// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/termbits-common.h`

use crate::tcflag_t;

pub type cc_t = u8;
pub type speed_t = u32;

/// `c_iflag` bits
/// Ignore break condition
pub const IGNBRK: tcflag_t = 0x001;
/// Signal interrupt on break
pub const BRKINT: tcflag_t = 0x002;
/// Ignore characters with parity errors
pub const IGNPAR: tcflag_t = 0x004;
/// Mark parity and framing errors
pub const PARMRK: tcflag_t = 0x008;
/// Enable input parity check
pub const INPCK: tcflag_t = 0x010;
/// Strip 8th bit off characters
pub const ISTRIP: tcflag_t = 0x020;
/// Map NL to CR on input
pub const INLCR: tcflag_t = 0x040;
/// Ignore CR
pub const IGNCR: tcflag_t = 0x080;
/// Map CR to NL on input
pub const ICRNL: tcflag_t = 0x100;
/// Any character will restart after stop
pub const IXANY: tcflag_t = 0x800;

/// `c_oflag` bits
// Perform output processing
pub const OPOST: tcflag_t = 0x01;
pub const OCRNL: tcflag_t = 0x08;
pub const ONOCR: tcflag_t = 0x10;
pub const ONLRET: tcflag_t = 0x20;
pub const OFILL: tcflag_t = 0x40;
pub const OFDEL: tcflag_t = 0x80;

/// `c_cflag` bit meaning
// Common CBAUD rates
// hang up
pub const B0: tcflag_t = 0x0000_0000;
pub const B50: tcflag_t = 0x0000_0001;
pub const B75: tcflag_t = 0x0000_0002;
pub const B110: tcflag_t = 0x0000_0003;
pub const B134: tcflag_t = 0x0000_0004;
pub const B150: tcflag_t = 0x0000_0005;
pub const B200: tcflag_t = 0x0000_0006;
pub const B300: tcflag_t = 0x0000_0007;
pub const B600: tcflag_t = 0x0000_0008;
pub const B1200: tcflag_t = 0x0000_0009;
pub const B1800: tcflag_t = 0x0000_000a;
pub const B2400: tcflag_t = 0x0000_000b;
pub const B4800: tcflag_t = 0x0000_000c;
pub const B9600: tcflag_t = 0x0000_000d;
pub const B19200: tcflag_t = 0x0000_000e;
pub const B38400: tcflag_t = 0x0000_000f;
pub const EXTA: tcflag_t = B19200;
pub const EXTB: tcflag_t = B38400;

/// address bit
pub const ADDRB: tcflag_t = 0x2000_0000;
/// mark or space (stick) parity
pub const CMSPAR: tcflag_t = 0x4000_0000;
/// flow control
pub const CRTSCTS: tcflag_t = 0x8000_0000;

/// Shift from CBAUD to CIBAUD
pub const IBSHIFT: tcflag_t = 16;

/// `tcflow()` ACTION argument and TCXONC use these
/// Suspend output
pub const TCOOFF: i32 = 0;
/// Restart suspended output
pub const TCOON: i32 = 1;
/// Send a STOP character
pub const TCIOFF: i32 = 2;
/// Send a START character
pub const TCION: i32 = 3;

/// `tcflush()` `QUEUE_SELECTOR` argument and TCFLSH use these
/// Discard data received but not yet read
pub const TCIFLUSH: i32 = 0;
/// Discard data written but not yet sent
pub const TCOFLUSH: i32 = 1;
/// Discard all pending data
pub const TCIOFLUSH: i32 = 2;
