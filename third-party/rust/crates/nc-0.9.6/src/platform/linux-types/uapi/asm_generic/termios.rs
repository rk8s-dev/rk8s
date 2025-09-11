// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/termios.h`
//!
//! Most architectures have straight copies of the x86 code, with
//! varying levels of bug fixes on top. Usually it's a good idea
//! to use this generic version instead, but be careful to avoid
//! ABI changes.
//!
//! New architectures should not provide their own version.

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct winsize_t {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

pub const NCC: usize = 8;

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct termio_t {
    /// input mode flags
    pub c_iflag: u16,

    /// output mode flags
    pub c_oflag: u16,

    /// control mode flags
    pub c_cflag: u16,

    /// local mode flags
    pub c_lflag: u16,

    /// line discipline
    pub c_line: u8,

    /// control characters
    pub c_cc: [u8; NCC],
}

/// modem lines
pub const TIOCM_LE: i32 = 0x001;
pub const TIOCM_DTR: i32 = 0x002;
pub const TIOCM_RTS: i32 = 0x004;
pub const TIOCM_ST: i32 = 0x008;
pub const TIOCM_SR: i32 = 0x010;
pub const TIOCM_CTS: i32 = 0x020;
pub const TIOCM_CAR: i32 = 0x040;
pub const TIOCM_RNG: i32 = 0x080;
pub const TIOCM_DSR: i32 = 0x100;
pub const TIOCM_CD: i32 = TIOCM_CAR;
pub const TIOCM_RI: i32 = TIOCM_RNG;
pub const TIOCM_OUT1: i32 = 0x2000;
pub const TIOCM_OUT2: i32 = 0x4000;
pub const TIOCM_LOOP: i32 = 0x8000;
