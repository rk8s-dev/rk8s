// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/ioctls.h`
//!
//! These are the most common definitions for tty ioctl numbers.
//! Most of them do not use the recommended `_IOC()`, but there is
//! probably some source code out there hardcoding the number,
//! so we might as well use them for all new platforms.
//!
//! The architectures that use different values here typically
//! try to be compatible with some Unix variants for the same
//! architecture.

use crate::{serial_iso7816_t, termios2_t, IO, IOR, IOW, IOWR};

/// 0x54 is just a magic number to make these relatively unique ('T')
pub const TCGETS: u32 = 0x5401;
pub const TCSETS: u32 = 0x5402;
pub const TCSETSW: u32 = 0x5403;
pub const TCSETSF: u32 = 0x5404;
pub const TCGETA: u32 = 0x5405;
pub const TCSETA: u32 = 0x5406;
pub const TCSETAW: u32 = 0x5407;
pub const TCSETAF: u32 = 0x5408;
pub const TCSBRK: u32 = 0x5409;
pub const TCXONC: u32 = 0x540A;
pub const TCFLSH: u32 = 0x540B;
pub const TIOCEXCL: u32 = 0x540C;
pub const TIOCNXCL: u32 = 0x540D;
pub const TIOCSCTTY: u32 = 0x540E;
pub const TIOCGPGRP: u32 = 0x540F;
pub const TIOCSPGRP: u32 = 0x5410;
pub const TIOCOUTQ: u32 = 0x5411;
pub const TIOCSTI: u32 = 0x5412;
pub const TIOCGWINSZ: u32 = 0x5413;
pub const TIOCSWINSZ: u32 = 0x5414;
pub const TIOCMGET: u32 = 0x5415;
pub const TIOCMBIS: u32 = 0x5416;
pub const TIOCMBIC: u32 = 0x5417;
pub const TIOCMSET: u32 = 0x5418;
pub const TIOCGSOFTCAR: u32 = 0x5419;
pub const TIOCSSOFTCAR: u32 = 0x541A;
pub const FIONREAD: u32 = 0x541B;
pub const TIOCINQ: u32 = FIONREAD;
pub const TIOCLINUX: u32 = 0x541C;
pub const TIOCCONS: u32 = 0x541D;
pub const TIOCGSERIAL: u32 = 0x541E;
pub const TIOCSSERIAL: u32 = 0x541F;
pub const TIOCPKT: u32 = 0x5420;
pub const FIONBIO: u32 = 0x5421;
pub const TIOCNOTTY: u32 = 0x5422;
pub const TIOCSETD: u32 = 0x5423;
pub const TIOCGETD: u32 = 0x5424;
/// Needed for POSIX `tcsendbreak()`
pub const TCSBRKP: u32 = 0x5425;
/// BSD compatibility
pub const TIOCSBRK: u32 = 0x5427;
/// BSD compatibility
pub const TIOCCBRK: u32 = 0x5428;
/// Return the session ID of FD
pub const TIOCGSID: u32 = 0x5429;
pub const TCGETS2: u32 = IOR::<termios2_t>(b'T', 0x2A);
pub const TCSETS2: u32 = IOW::<termios2_t>(b'T', 0x2B);
pub const TCSETSW2: u32 = IOW::<termios2_t>(b'T', 0x2C);
pub const TCSETSF2: u32 = IOW::<termios2_t>(b'T', 0x2D);
pub const TIOCGRS485: u32 = 0x542E;
pub const TIOCSRS485: u32 = 0x542F;
/// Get Pty Number (of pty-mux device)
pub const TIOCGPTN: u32 = IOR::<u32>(b'T', 0x30);
/// Lock/unlock Pty
pub const TIOCSPTLCK: u32 = IOW::<u32>(b'T', 0x31);
/// Get primary device node of /dev/console
pub const TIOCGDEV: u32 = IOR::<u32>(b'T', 0x32);
/// SYS5 TCGETX compatibility
pub const TCGETX: u32 = 0x5432;
pub const TCSETX: u32 = 0x5433;
pub const TCSETXF: u32 = 0x5434;
pub const TCSETXW: u32 = 0x5435;
/// pty: generate signal
pub const TIOCSIG: u32 = IOW::<u32>(b'T', 0x36);
pub const TIOCVHANGUP: u32 = 0x5437;
/// Get packet mode state
pub const TIOCGPKT: u32 = IOR::<u32>(b'T', 0x38);
/// Get Pty lock state
pub const TIOCGPTLCK: u32 = IOR::<u32>(b'T', 0x39);
/// Get exclusive mode state
pub const TIOCGEXCL: u32 = IOR::<u32>(b'T', 0x40);
/// Safely open the slave
pub const TIOCGPTPEER: u32 = IO(b'T', 0x41);
pub const TIOCGISO7816: u32 = IOR::<serial_iso7816_t>(b'T', 0x42);
pub const TIOCSISO7816: u32 = IOWR::<serial_iso7816_t>(b'T', 0x43);

pub const FIONCLEX: u32 = 0x5450;
pub const FIOCLEX: u32 = 0x5451;
pub const FIOASYNC: u32 = 0x5452;
pub const TIOCSERCONFIG: u32 = 0x5453;
pub const TIOCSERGWILD: u32 = 0x5454;
pub const TIOCSERSWILD: u32 = 0x5455;
pub const TIOCGLCKTRMIOS: u32 = 0x5456;
pub const TIOCSLCKTRMIOS: u32 = 0x5457;
/// For debugging only
pub const TIOCSERGSTRUCT: u32 = 0x5458;
/// Get line status register
pub const TIOCSERGETLSR: u32 = 0x5459;
/// Get multiport config
pub const TIOCSERGETMULTI: u32 = 0x545A;
/// Set multiport config
pub const TIOCSERSETMULTI: u32 = 0x545B;

/// wait for a change on serial input line(s)
pub const TIOCMIWAIT: u32 = 0x545C;
/// read serial port inline interrupt counts
pub const TIOCGICOUNT: u32 = 0x545D;

/// Some arches already define FIOQSIZE due to a historical
/// conflict with a Hayes modem-specific ioctl value.
#[cfg(not(target_arch = "arm"))]
pub const FIOQSIZE: u32 = 0x5460;

/// Used for packet mode
pub const TIOCPKT_DATA: u32 = 0;
pub const TIOCPKT_FLUSHREAD: u32 = 1;
pub const TIOCPKT_FLUSHWRITE: u32 = 2;
pub const TIOCPKT_STOP: u32 = 4;
pub const TIOCPKT_START: u32 = 8;
pub const TIOCPKT_NOSTOP: u32 = 16;
pub const TIOCPKT_DOSTOP: u32 = 32;
pub const TIOCPKT_IOCTL: u32 = 64;

/// Transmitter physically empty
pub const TIOCSER_TEMT: u32 = 0x01;
