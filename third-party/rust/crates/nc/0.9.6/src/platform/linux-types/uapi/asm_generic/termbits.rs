// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/termbits.h`

use crate::{cc_t, speed_t};

pub type tcflag_t = u32;

pub const NCCS: usize = 19;
// NOTE(Shaohua): It's strange that NCCS is defined in glibc and musl with
// value 32, which is not present in kernel header.
// See: https://github.com/dcuddeback/termios-rs/issues/9
//pub const NCCS: usize = 32;

#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct termios_t {
    /// input mode flags
    pub c_iflag: tcflag_t,

    /// output mode flags
    pub c_oflag: tcflag_t,

    /// control mode flags
    pub c_cflag: tcflag_t,

    /// local mode flags
    pub c_lflag: tcflag_t,

    /// line discipline
    pub c_line: cc_t,

    /// control characters
    pub c_cc: [cc_t; NCCS],
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct termios2_t {
    /// input mode flags
    pub c_iflag: tcflag_t,

    /// output mode flags
    pub c_oflag: tcflag_t,

    /// control mode flags
    pub c_cflag: tcflag_t,

    /// local mode flags
    pub c_lflag: tcflag_t,

    /// line discipline
    pub c_line: cc_t,

    /// control characters
    pub c_cc: [cc_t; NCCS],

    /// input speed
    pub c_ispeed: speed_t,

    /// output speed
    pub c_ospeed: speed_t,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct ktermios_t {
    /// input mode flags
    pub c_iflag: tcflag_t,

    /// output mode flags
    pub c_oflag: tcflag_t,

    /// control mode flags
    pub c_cflag: tcflag_t,

    /// local mode flags
    pub c_lflag: tcflag_t,

    /// line discipline
    pub c_line: cc_t,

    /// control characters
    pub c_cc: [cc_t; NCCS],

    /// input speed
    pub c_ispeed: speed_t,

    /// output speed
    pub c_ospeed: speed_t,
}

/// `c_cc` characters
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VERASE: usize = 2;
pub const VKILL: usize = 3;
pub const VEOF: usize = 4;
pub const VTIME: usize = 5;
pub const VMIN: usize = 6;
pub const VSWTC: usize = 7;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VSUSP: usize = 10;
pub const VEOL: usize = 11;
pub const VREPRINT: usize = 12;
pub const VDISCARD: usize = 13;
pub const VWERASE: usize = 14;
pub const VLNEXT: usize = 15;
pub const VEOL2: usize = 16;

/// `c_iflag` bits
pub const IUCLC: tcflag_t = 0x02_000;
pub const IXON: tcflag_t = 0x04_000;
pub const IXOFF: tcflag_t = 0x10_000;
pub const IMAXBEL: tcflag_t = 0x20_000;
pub const IUTF8: tcflag_t = 0x40_000;

/// `c_oflag` bits
pub const OLCUC: tcflag_t = 0x00002;
pub const ONLCR: tcflag_t = 0x00004;
pub const NLDLY: tcflag_t = 0x00100;
pub const NL0: tcflag_t = 0x00000;
pub const NL1: tcflag_t = 0x00100;
pub const CRDLY: tcflag_t = 0x00600;
pub const CR0: tcflag_t = 0x00000;
pub const CR1: tcflag_t = 0x00200;
pub const CR2: tcflag_t = 0x00400;
pub const CR3: tcflag_t = 0x00600;
pub const TABDLY: tcflag_t = 0x01800;
pub const TAB0: tcflag_t = 0x00000;
pub const TAB1: tcflag_t = 0x00800;
pub const TAB2: tcflag_t = 0x01000;
pub const TAB3: tcflag_t = 0x01800;
pub const XTABS: tcflag_t = 0x01800;
pub const BSDLY: tcflag_t = 0x02000;
pub const BS0: tcflag_t = 0x00000;
pub const BS1: tcflag_t = 0x02000;
pub const VTDLY: tcflag_t = 0x04000;
pub const VT0: tcflag_t = 0x00000;
pub const VT1: tcflag_t = 0x04000;
pub const FFDLY: tcflag_t = 0x08000;
pub const FF0: tcflag_t = 0x00000;
pub const FF1: tcflag_t = 0x08000;

/// `c_cflag` bit meaning
pub const CBAUD: tcflag_t = 0x0000_100f;
pub const CSIZE: tcflag_t = 0x0000_0030;
pub const CS5: tcflag_t = 0x0000_0000;
pub const CS6: tcflag_t = 0x0000_0010;
pub const CS7: tcflag_t = 0x0000_0020;
pub const CS8: tcflag_t = 0x0000_0030;
pub const CSTOPB: tcflag_t = 0x0000_0040;
pub const CREAD: tcflag_t = 0x0000_0080;
pub const PARENB: tcflag_t = 0x0000_0100;
pub const PARODD: tcflag_t = 0x0000_0200;
pub const HUPCL: tcflag_t = 0x0000_0400;
pub const CLOCAL: tcflag_t = 0x0000_0800;
pub const CBAUDEX: tcflag_t = 0x0000_1000;
pub const BOTHER: tcflag_t = 0x0000_1000;
pub const B57600: tcflag_t = 0x0000_1001;
pub const B115200: tcflag_t = 0x0000_1002;
pub const B230400: tcflag_t = 0x0000_1003;
pub const B460800: tcflag_t = 0x0000_1004;
pub const B500000: tcflag_t = 0x0000_1005;
pub const B576000: tcflag_t = 0x0000_1006;
pub const B921600: tcflag_t = 0x0000_1007;
pub const B1000000: tcflag_t = 0x0000_1008;
pub const B1152000: tcflag_t = 0x0000_1009;
pub const B1500000: tcflag_t = 0x0000_100a;
pub const B2000000: tcflag_t = 0x0000_100b;
pub const B2500000: tcflag_t = 0x0000_100c;
pub const B3000000: tcflag_t = 0x0000_100d;
pub const B3500000: tcflag_t = 0x0000_100e;
pub const B4000000: tcflag_t = 0x0000_100f;
/// input baud rate;
pub const CIBAUD: tcflag_t = 0x100f_0000;

/// `c_lflag` bits
pub const ISIG: tcflag_t = 0x00001;
pub const ICANON: tcflag_t = 0x00002;
pub const XCASE: tcflag_t = 0x00004;
pub const ECHO: tcflag_t = 0x00008;
pub const ECHOE: tcflag_t = 0x00010;
pub const ECHOK: tcflag_t = 0x00020;
pub const ECHONL: tcflag_t = 0x00040;
pub const NOFLSH: tcflag_t = 0x00080;
pub const TOSTOP: tcflag_t = 0x00100;
pub const ECHOCTL: tcflag_t = 0x00200;
pub const ECHOPRT: tcflag_t = 0x00400;
pub const ECHOKE: tcflag_t = 0x00800;
pub const FLUSHO: tcflag_t = 0x01000;
pub const PENDIN: tcflag_t = 0x04000;
pub const IEXTEN: tcflag_t = 0x08000;
pub const EXTPROC: tcflag_t = 0x10000;

/// tcsetattr uses these
pub const TCSANOW: u32 = 0;
pub const TCSADRAIN: u32 = 1;
pub const TCSAFLUSH: u32 = 2;
