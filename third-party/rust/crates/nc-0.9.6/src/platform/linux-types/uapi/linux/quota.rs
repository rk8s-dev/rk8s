// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/quota.h`

pub const MAXQUOTAS: i32 = 3;
/// element used for user quotas
pub const USRQUOTA: i32 = 0;
/// element used for group quotas
pub const GRPQUOTA: i32 = 1;
/// element used for project quotas
pub const PRJQUOTA: i32 = 2;

// Definitions for the default names of the quotas files.
pub const INITQFNAMES: &[&str] = &[
    // USRQUOTA
    "user",
    // GRPQUOTA
    "group",
    // PRJQUOTA
    "project",
    "undefined",
];

/// Command definitions for the 'quotactl' system call.
///
/// The commands are broken into a main command defined below
/// and a subcommand that is used to convey the type of
/// quota that is being manipulated (see above).
pub const SUBCMDMASK: i32 = 0x00ff;
pub const SUBCMDSHIFT: i32 = 8;

#[inline]
#[must_use]
pub const fn QCMD(cmd: i32, type_: i32) -> i32 {
    (cmd << SUBCMDSHIFT) | (type_ & SUBCMDMASK)
}

/// sync disk copy of a filesystems quotas
pub const Q_SYNC: i32 = 0x0080_0001;
/// turn quotas on
pub const Q_QUOTAON: i32 = 0x0080_0002;
/// turn quotas off
pub const Q_QUOTAOFF: i32 = 0x0080_0003;
/// get quota format used on given filesystem
pub const Q_GETFMT: i32 = 0x0080_0004;
/// get information about quota files
pub const Q_GETINFO: i32 = 0x0080_0005;
/// set information about quota files
pub const Q_SETINFO: i32 = 0x0080_0006;
/// get user quota structure
pub const Q_GETQUOTA: i32 = 0x0080_0007;
/// set user quota structure
pub const Q_SETQUOTA: i32 = 0x0080_0008;
/// get disk limits and usage >= ID
pub const Q_GETNEXTQUOTA: i32 = 0x0080_0009;

/// Quota format type IDs
pub const QFMT_VFS_OLD: i32 = 1;
pub const QFMT_VFS_V0: i32 = 2;
pub const QFMT_OCFS2: i32 = 3;
pub const QFMT_VFS_V1: i32 = 4;

/// Size of block in which space limits are passed through the quota interface
pub const QIF_DQBLKSIZE_BITS: i32 = 10;
pub const QIF_DQBLKSIZE: i32 = 1 << QIF_DQBLKSIZE_BITS;

/// Quota structure used for communication with userspace via quotactl
/// Following flags are used to specify which fields are valid
pub const QIF_BLIMITS_B: i32 = 0;
pub const QIF_SPACE_B: i32 = 1;
pub const QIF_ILIMITS_B: i32 = 2;
pub const QIF_INODES_B: i32 = 3;
pub const QIF_BTIME_B: i32 = 4;
pub const QIF_ITIME_B: i32 = 5;

pub const QIF_BLIMITS: i32 = 1 << QIF_BLIMITS_B;
pub const QIF_SPACE: i32 = 1 << QIF_SPACE_B;
pub const QIF_ILIMITS: i32 = 1 << QIF_ILIMITS_B;
pub const QIF_INODES: i32 = 1 << QIF_INODES_B;
pub const QIF_BTIME: i32 = 1 << QIF_BTIME_B;
pub const QIF_ITIME: i32 = 1 << QIF_ITIME_B;
pub const QIF_LIMITS: i32 = QIF_BLIMITS | QIF_ILIMITS;
pub const QIF_USAGE: i32 = QIF_SPACE | QIF_INODES;
pub const QIF_TIMES: i32 = QIF_BTIME | QIF_ITIME;
pub const QIF_ALL: i32 = QIF_LIMITS | QIF_USAGE | QIF_TIMES;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct if_dqblk_t {
    pub dqb_bhardlimit: u64,
    pub dqb_bsoftlimit: u64,
    pub dqb_curspace: u64,
    pub dqb_ihardlimit: u64,
    pub dqb_isoftlimit: u64,
    pub dqb_curinodes: u64,
    pub dqb_btime: u64,
    pub dqb_itime: u64,
    pub dqb_valid: u64,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct if_nextdqblk_t {
    pub dqb_bhardlimit: u64,
    pub dqb_bsoftlimit: u64,
    pub dqb_curspace: u64,
    pub dqb_ihardlimit: u64,
    pub dqb_isoftlimit: u64,
    pub dqb_curinodes: u64,
    pub dqb_btime: u64,
    pub dqb_itime: u64,
    pub dqb_valid: u32,
    pub dqb_id: u32,
}

/// Structure used for setting quota information about file via quotactl
/// Following flags are used to specify which fields are valid
pub const IIF_BGRACE: i32 = 1;
pub const IIF_IGRACE: i32 = 2;
pub const IIF_FLAGS: i32 = 4;
pub const IIF_ALL: i32 = IIF_BGRACE | IIF_IGRACE | IIF_FLAGS;

pub const DQF_ROOT_SQUASH_B: i32 = 0;
pub const DQF_SYS_FILE_B: i32 = 16;
/// Kernel internal flags invisible to userspace
pub const DQF_PRIVATE: i32 = 17;

/// Root squash enabled (for v1 quota format)
pub const DQF_ROOT_SQUASH: i32 = 1 << DQF_ROOT_SQUASH_B;
/// Quota stored in a system file
pub const DQF_SYS_FILE: i32 = 1 << DQF_SYS_FILE_B;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct if_dqinfo_t {
    pub dqi_bgrace: u64,
    pub dqi_igrace: u64,

    /// DFQ_*
    pub dqi_flags: u32,

    pub dqi_valid: u32,
}

/// Definitions for quota netlink interface
pub const QUOTA_NL_NOWARN: i32 = 0;
/// Inode hardlimit reached
pub const QUOTA_NL_IHARDWARN: i32 = 1;
/// Inode grace time expired
pub const QUOTA_NL_ISOFTLONGWARN: i32 = 2;
/// Inode softlimit reached
pub const QUOTA_NL_ISOFTWARN: i32 = 3;
/// Block hardlimit reached
pub const QUOTA_NL_BHARDWARN: i32 = 4;
/// Block grace time expired
pub const QUOTA_NL_BSOFTLONGWARN: i32 = 5;
/// Block softlimit reached
pub const QUOTA_NL_BSOFTWARN: i32 = 6;
/// Usage got below inode hardlimit
pub const QUOTA_NL_IHARDBELOW: i32 = 7;
/// Usage got below inode softlimit
pub const QUOTA_NL_ISOFTBELOW: i32 = 8;
/// Usage got below block hardlimit
pub const QUOTA_NL_BHARDBELOW: i32 = 9;
/// Usage got below block softlimit
pub const QUOTA_NL_BSOFTBELOW: i32 = 10;

pub const QUOTA_NL_C_UNSPEC: i32 = 0;
pub const QUOTA_NL_C_WARNING: i32 = 1;
pub const __QUOTA_NL_C_MAX: i32 = 2;
pub const QUOTA_NL_C_MAX: i32 = __QUOTA_NL_C_MAX - 1;

pub const QUOTA_NL_A_UNSPEC: i32 = 0;
pub const QUOTA_NL_A_QTYPE: i32 = 1;
pub const QUOTA_NL_A_EXCESS_ID: i32 = 2;
pub const QUOTA_NL_A_WARNING: i32 = 3;
pub const QUOTA_NL_A_DEV_MAJOR: i32 = 4;
pub const QUOTA_NL_A_DEV_MINOR: i32 = 5;
pub const QUOTA_NL_A_CAUSED_ID: i32 = 6;
pub const QUOTA_NL_A_PAD: i32 = 7;
pub const __QUOTA_NL_A_MAX: i32 = 8;
pub const QUOTA_NL_A_MAX: i32 = __QUOTA_NL_A_MAX - 1;
