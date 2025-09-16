// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/reboot.h`

/// Arguments to reboot system call.  These are passed to
/// the boot program and on to init.
/// flags for system auto-booting itself
pub const RB_AUTOBOOT: i32 = 0;

/// force prompt of device of root filesystem
pub const RB_ASKNAME: i32 = 0x001;
/// reboot to single user only
pub const RB_SINGLE: i32 = 0x002;
/// dont sync before reboot
pub const RB_NOSYNC: i32 = 0x004;
/// don't reboot, just halt
pub const RB_HALT: i32 = 0x008;
/// Unused placeholder to specify init path
pub const RB_INITNAME: i32 = 0x010;
/// use compiled-in rootdev
pub const RB_DFLTROOT: i32 = 0x020;
/// give control to kernel debugger
pub const RB_KDB: i32 = 0x040;
/// mount root fs read-only
pub const RB_RDONLY: i32 = 0x080;
/// dump kernel memory before reboot
pub const RB_DUMP: i32 = 0x100;
/// Unused placeholder
pub const RB_MINIROOT: i32 = 0x200;
/// print all potentially useful info
pub const RB_VERBOSE: i32 = 0x800;
/// use serial port as console
pub const RB_SERIAL: i32 = 0x1000;
/// use cdrom as root
pub const RB_CDROM: i32 = 0x2000;
/// turn the power off if possible
pub const RB_POWEROFF: i32 = 0x4000;
/// use GDB remote debugger instead of DDB
pub const RB_GDB: i32 = 0x8000;
/// start up with the console muted
pub const RB_MUTE: i32 = 0x10000;
/// unused placeholder
pub const RB_SELFTEST: i32 = 0x20000;
/// reserved for internal use of boot blocks
pub const RB_RESERVED1: i32 = 0x40000;
/// reserved for internal use of boot blocks
pub const RB_RESERVED2: i32 = 0x80000;
/// pause after each output line during probe
pub const RB_PAUSE: i32 = 0x10_0000;
/// unmount the rootfs and mount it again
pub const RB_REROOT: i32 = 0x20_0000;
/// Power cycle if possible
pub const RB_POWERCYCLE: i32 = 0x40_0000;
/// Probe multiple consoles
pub const RB_PROBE: i32 = 0x1000_0000;
/// use multiple consoles
pub const RB_MULTIPLE: i32 = 0x2000_0000;

/// have `struct bootinfo *` arg
#[allow(overflowing_literals)]
pub const RB_BOOTINFO: i32 = 0x8000_0000;
