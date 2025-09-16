// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/capability.h`

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]

use core::mem::size_of;

use crate::le32_t;

/// User-level do most of the mapping between kernel and user
/// capabilities based on the version tag given by the kernel.
///
/// The kernel might be somewhat backwards compatible, but don't bet on it.
///
/// Note, `cap_t`, is defined by POSIX (draft) to be an "opaque" pointer to
/// a set of three capability sets.  The transposition of 3*the
/// following structure to such a composite is better handled in a user
/// library since the draft standard requires the use of malloc/free etc..
pub const LINUX_CAPABILITY_VERSION_1: i32 = 0x1998_0330;
pub const LINUX_CAPABILITY_U32S_1: i32 = 1;

/// deprecated - use v3
pub const LINUX_CAPABILITY_VERSION_2: i32 = 0x2007_1026;
pub const LINUX_CAPABILITY_U32S_2: i32 = 2;

pub const LINUX_CAPABILITY_VERSION_3: i32 = 0x2008_0522;
pub const LINUX_CAPABILITY_U32S_3: i32 = 2;

#[repr(C)]
#[derive(Debug, Default)]
pub struct cap_user_header_t {
    pub version: u32,
    pub pid: i32,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct cap_user_data_t {
    pub effective: u32,
    pub permitted: u32,
    pub inheritable: u32,
}

#[allow(overflowing_literals)]
pub const VFS_CAP_REVISION_MASK: i32 = 0xff00_0000;
pub const VFS_CAP_REVISION_SHIFT: i32 = 24;
// TODO(Shaohua):
//#define VFS_CAP_FLAGS_MASK	~VFS_CAP_REVISION_MASK
pub const VFS_CAP_FLAGS_EFFECTIVE: i32 = 0x0000_0001;

pub const VFS_CAP_REVISION_1: i32 = 0x0100_0000;
pub const VFS_CAP_U32_1: i32 = 1;
pub const XATTR_CAPS_SZ_1: i32 = (size_of::<le32_t>() as i32) * (1 + 2 * VFS_CAP_U32_1);

pub const VFS_CAP_REVISION_2: i32 = 0x0200_0000;
pub const VFS_CAP_U32_2: i32 = 2;
pub const XATTR_CAPS_SZ_2: i32 = (size_of::<le32_t>() as i32) * (1 + 2 * VFS_CAP_U32_2);

pub const VFS_CAP_REVISION_3: i32 = 0x0300_0000;
pub const VFS_CAP_U32_3: i32 = 2;
pub const XATTR_CAPS_SZ_3: i32 = (size_of::<le32_t>() as i32) * (2 + 2 * VFS_CAP_U32_3);

pub const XATTR_CAPS_SZ: i32 = XATTR_CAPS_SZ_3;
pub const VFS_CAP_U32: i32 = VFS_CAP_U32_3;
pub const VFS_CAP_REVISION: i32 = VFS_CAP_REVISION_3;

/// Backwardly compatible definition for source code - trapped in a
/// 32-bit world. If you find you need this, please consider using
/// libcap to untrap yourself...
pub const LINUX_CAPABILITY_VERSION: i32 = LINUX_CAPABILITY_VERSION_1;
pub const LINUX_CAPABILITY_U32S: i32 = LINUX_CAPABILITY_U32S_1;

/// POSIX-draft defined capabilities.
///
/// In a system with the `_POSIX_CHOWN_RESTRICTED` option defined, this
/// overrides the restriction of changing file ownership and group ownership.
pub const CAP_CHOWN: i32 = 0;

/// Override all DAC access, including ACL execute access if
/// `_POSIX_ACL` is defined. Excluding DAC access covered by
/// `CAP_LINUX_IMMUTABLE`.
pub const CAP_DAC_OVERRIDE: i32 = 1;

/// Overrides all DAC restrictions regarding read and search on files
/// and directories, including ACL restrictions if `_POSIX_ACL` is
/// defined. Excluding DAC access covered by `CAP_LINUX_IMMUTABLE`.
pub const CAP_DAC_READ_SEARCH: i32 = 2;

/// Overrides all restrictions about allowed operations on files, where
/// file owner ID must be equal to the user ID, except where `CAP_FSETID`
/// is applicable. It doesn't override MAC and DAC restrictions.
pub const CAP_FOWNER: i32 = 3;

/// Overrides the following restrictions that the effective user ID
/// shall match the file owner ID when setting the `S_ISUID` and `S_ISGID`
/// bits on that file.
///
/// That the effective group ID (or one of the supplementary group IDs)
/// shall match the file owner ID when setting the `S_ISGID` bit on that file;
/// that the `S_ISUID` and `S_ISGID` bits are cleared on successful return
/// from chown(2) (not implemented).
pub const CAP_FSETID: i32 = 4;

/// Overrides the restriction that the real or effective user ID of a
/// process sending a signal must match the real or effective user ID
/// of the process receiving the signal.
pub const CAP_KILL: i32 = 5;

/// Allows setgid(2) manipulation
/// Allows setgroups(2)
/// Allows forged gids on socket credentials passing.
pub const CAP_SETGID: i32 = 6;

/// Allows set*uid(2) manipulation (including fsuid).
/// Allows forged pids on socket credentials passing.
pub const CAP_SETUID: i32 = 7;

/// Linux-specific capabilities
///
/// Without VFS support for capabilities:
///   Transfer any capability in your permitted set to any pid,
///   remove any capability in your permitted set from any pid
/// With VFS support for capabilities (neither of above, but)
///   Add any capability from current's capability bounding set
///     to the current process' inheritable set
///   Allow taking bits out of capability bounding set
///   Allow modification of the securebits for a process
pub const CAP_SETPCAP: i32 = 8;

/// Allow modification of `S_IMMUTABLE` and `S_APPEND` file attributes
pub const CAP_LINUX_IMMUTABLE: i32 = 9;

/// Allows binding to TCP/UDP sockets below 1024
/// Allows binding to ATM VCIs below 32
pub const CAP_NET_BIND_SERVICE: i32 = 10;

/// Allow broadcasting, listen to multicast
pub const CAP_NET_BROADCAST: i32 = 11;

/// Network administration capacity.
///
/// Allow interface configuration
/// Allow administration of IP firewall, masquerading and accounting
/// Allow setting debug option on sockets
/// Allow modification of routing tables
/// Allow setting arbitrary process / process group ownership on sockets
/// Allow binding to any address for transparent proxying (also via `NET_RAW`)
/// Allow setting TOS (type of service)
/// Allow setting promiscuous mode
/// Allow clearing driver statistics
/// Allow multicasting
/// Allow read/write of device-specific registers
/// Allow activation of ATM control sockets
pub const CAP_NET_ADMIN: i32 = 12;

/// Allow use of RAW sockets
/// Allow use of PACKET sockets
/// Allow binding to any address for transparent proxying (also via `NET_ADMIN`)
pub const CAP_NET_RAW: i32 = 13;

/// Allow locking of shared memory segments
/// Allow mlock and mlockall (which doesn't really have anything to do with IPC)
pub const CAP_IPC_LOCK: i32 = 14;

/// Override IPC ownership checks
pub const CAP_IPC_OWNER: i32 = 15;

/// Insert and remove kernel modules - modify kernel without limit
pub const CAP_SYS_MODULE: i32 = 16;

/// Allow ioperm/iopl access
/// Allow sending USB messages to any device via `/dev/bus/usb`
pub const CAP_SYS_RAWIO: i32 = 17;

/// Allow use of `chroot()`
pub const CAP_SYS_CHROOT: i32 = 18;

/// Allow `ptrace()` of any process
pub const CAP_SYS_PTRACE: i32 = 19;

/// Allow configuration of process accounting
pub const CAP_SYS_PACCT: i32 = 20;

/// Administration capacity.
///
/// Allow configuration of the secure attention key
/// Allow administration of the random device
/// Allow examination and configuration of disk quotas
/// Allow setting the domainname
/// Allow setting the hostname
/// Allow calling `bdflush()`
/// Allow `mount()` and `umount()`, setting up new smb connection
/// Allow some autofs root ioctls
/// Allow nfsservctl
/// Allow `VM86_REQUEST_IRQ`
/// Allow to read/write pci config on alpha
/// Allow `irix_prctl` on mips (setstacksize)
/// Allow flushing all cache on m68k (`sys_cacheflush`)
/// Allow removing semaphores
/// Used instead of `CAP_CHOWN` to "chown" IPC message queues, semaphores and shared memory
/// Allow locking/unlocking of shared memory segment
/// Allow turning swap on/off
/// Allow forged pids on socket credentials passing
/// Allow setting readahead and flushing buffers on block devices
/// Allow setting geometry in floppy driver
/// Allow turning DMA on/off in xd driver
/// Allow administration of md devices (mostly the above, but some extra ioctls)
/// Allow tuning the ide driver
/// Allow access to the nvram device
/// Allow administration of `apm_bios`, serial and bttv (TV) device
/// Allow manufacturer commands in isdn CAPI support driver
/// Allow reading non-standardized portions of pci configuration space
/// Allow DDI debug ioctl on sbpcd driver
/// Allow setting up serial ports
/// Allow sending raw qic-117 commands
/// Allow enabling/disabling tagged queuing on SCSI controllers and sending arbitrary SCSI commands
/// Allow setting encryption key on loopback filesystem
/// Allow setting zone reclaim policy
pub const CAP_SYS_ADMIN: i32 = 21;

/// Allow use of `reboot()`
pub const CAP_SYS_BOOT: i32 = 22;

/// Allow raising priority and setting priority on other (different UID) processes.
///
/// Allow use of FIFO and round-robin (realtime) scheduling on own
/// processes and setting the scheduling algorithm used by another process.
///
/// Allow setting cpu affinity on other processes
pub const CAP_SYS_NICE: i32 = 23;

/// Override resource limits. Set resource limits.
///
/// Override quota limits.
///
/// Override reserved space on ext2 filesystem.
///
/// Modify data journaling mode on ext3 filesystem (uses journaling resources)
/// NOTE: ext2 honors fsuid when checking for resource overrides, so you can
/// override using fsuid too.
///
/// Override size restrictions on IPC message queues.
///
/// Allow more than 64hz interrupts from the real-time clock.
///
/// Override max number of consoles on console allocation.
///
/// Override max number of keymaps
pub const CAP_SYS_RESOURCE: i32 = 24;

/// Allow manipulation of system clock.
///
/// Allow `irix_stime` on mips.
///
/// Allow setting the real-time clock.
pub const CAP_SYS_TIME: i32 = 25;

/// Allow configuration of tty devices.
///
/// Allow `vhangup()` of tty.
pub const CAP_SYS_TTY_CONFIG: i32 = 26;

/// Allow the privileged aspects of `mknod()`.
pub const CAP_MKNOD: i32 = 27;

/// Allow taking of leases on files.
pub const CAP_LEASE: i32 = 28;

/// Allow writing the audit log via unicast netlink socket.
pub const CAP_AUDIT_WRITE: i32 = 29;

/// Allow configuration of audit via unicast netlink socket.
pub const CAP_AUDIT_CONTROL: i32 = 30;

pub const CAP_SETFCAP: i32 = 31;

/// Override MAC access.
///
/// The base kernel enforces no MAC policy.
///
/// An LSM may enforce a MAC policy, and if it does and it chooses
/// to implement capability based overrides of that policy, this is
/// the capability it should use to do so.
pub const CAP_MAC_OVERRIDE: i32 = 32;

/// Allow MAC configuration or state changes.
///
/// The base kernel requires no MAC configuration.
///
/// An LSM may enforce a MAC policy, and if it does and it chooses
/// to implement capability based checks on modifications to that
/// policy or the data required to maintain it, this is the
/// capability it should use to do so.
pub const CAP_MAC_ADMIN: i32 = 33;

/// Allow configuring the kernel's syslog (printk behaviour).
pub const CAP_SYSLOG: i32 = 34;

/// Allow triggering something that will wake the system.
pub const CAP_WAKE_ALARM: i32 = 35;

/// Allow preventing system suspends.
pub const CAP_BLOCK_SUSPEND: i32 = 36;

/// Allow reading the audit log via multicast netlink socket.
pub const CAP_AUDIT_READ: i32 = 37;

pub const CAP_LAST_CAP: i32 = CAP_AUDIT_READ;

// TODO(Shaohua):
//#define cap_valid(x) ((x) >= 0 && (x) <= CAP_LAST_CAP)

/*
 * Bit location of each capability (used by user-space library and kernel)
 */

//#define CAP_TO_INDEX(x)     ((x) >> 5)        /* 1 << 5 == bits in __u32 */
//#define CAP_TO_MASK(x)      (1 << ((x) & 31)) /* mask for indexed __u32 */
