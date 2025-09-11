// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/xattr.h`

/// set value, fail if attr already exists
pub const XATTR_CREATE: i32 = 0x1;

/// set value, fail if attr does not exist
pub const XATTR_REPLACE: i32 = 0x2;

/// Namespaces
pub const XATTR_OS2_PREFIX: &str = "os2.";

pub const XATTR_MAC_OSX_PREFIX: &str = "osx.";

pub const XATTR_BTRFS_PREFIX: &str = "btrfs.";

pub const XATTR_HURD_PREFIX: &str = "gnu.";

pub const XATTR_SECURITY_PREFIX: &str = "security.";

pub const XATTR_SYSTEM_PREFIX: &str = "system.";

pub const XATTR_TRUSTED_PREFIX: &str = "trusted.";

pub const XATTR_USER_PREFIX: &str = "user.";

/// Security namespace
pub const XATTR_EVM_SUFFIX: &str = "evm";

pub const XATTR_IMA_SUFFIX: &str = "ima";
// TODO(Shaohua): Fix concat!() macro
//pub const XATTR_NAME_IMA: &str = XATTR_SECURITY_PREFIX + XATTR_IMA_SUFFIX;

pub const XATTR_SELINUX_SUFFIX: &str = "selinux";
//pub const XATTR_NAME_SELINUX: &str = XATTR_SECURITY_PREFIX + XATTR_SELINUX_SUFFIX;

pub const XATTR_SMACK_SUFFIX: &str = "SMACK64";
pub const XATTR_SMACK_IPIN: &str = "SMACK64IPIN";
pub const XATTR_SMACK_IPOUT: &str = "SMACK64IPOUT";
pub const XATTR_SMACK_EXEC: &str = "SMACK64EXEC";
pub const XATTR_SMACK_TRANSMUTE: &str = "SMACK64TRANSMUTE";
pub const XATTR_SMACK_MMAP: &str = "SMACK64MMAP";
//pub const XATTR_NAME_SMACK: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_SUFFIX;
//pub const XATTR_NAME_SMACKIPIN: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_IPIN;
//pub const XATTR_NAME_SMACKIPOUT: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_IPOUT;
//pub const XATTR_NAME_SMACKEXEC: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_EXEC;
//pub const XATTR_NAME_SMACKTRANSMUTE: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_TRANSMUTE;
//pub const XATTR_NAME_SMACKMMAP: &str = XATTR_SECURITY_PREFIX + XATTR_SMACK_MMAP;

pub const XATTR_APPARMOR_SUFFIX: &str = "apparmor";
//pub const XATTR_NAME_APPARMOR: &str = XATTR_SECURITY_PREFIX + XATTR_APPARMOR_SUFFIX;

pub const XATTR_CAPS_SUFFIX: &str = "capability";
//pub const XATTR_NAME_CAPS: &str = XATTR_SECURITY_PREFIX + XATTR_CAPS_SUFFIX;

pub const XATTR_POSIX_ACL_ACCESS: &str = "posix_acl_access";
//pub const XATTR_NAME_POSIX_ACL_ACCESS: &str = XATTR_SYSTEM_PREFIX + XATTR_POSIX_ACL_ACCESS;
pub const XATTR_POSIX_ACL_DEFAULT: &str = "posix_acl_default";
//pub const XATTR_NAME_POSIX_ACL_DEFAULT: &str =
//    XATTR_SYSTEM_PREFIX + XATTR_POSIX_ACL_DEFAULT;
