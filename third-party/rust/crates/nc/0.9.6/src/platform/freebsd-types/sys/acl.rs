// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/acl.h`

use crate::{uid_t, EXTATTR_NAMESPACE_SYSTEM, S_IRWXG, S_IRWXO, S_IRWXU};

pub type acl_tag_t = u32;
pub type acl_perm_t = u32;
pub type acl_entry_type_t = u16;
pub type acl_flag_t = u16;
pub type acl_type_t = i32;
pub type acl_permset_t = *mut i32;
pub type acl_flagset_t = *mut u16;

/*
 * With 254 entries, "struct acl_t_struct" is exactly one 4kB page big.
 * Note that with NFSv4 ACLs, the maximum number of ACL entries one
 * may set on file or directory is about half of ACL_MAX_ENTRIES.
 *
 * If you increase this, you might also need to increase
 * _ACL_T_ALIGNMENT_BITS in lib/libc/posix1e/acl_support.h.
 *
 * The maximum number of POSIX.1e ACLs is controlled
 * by OLDACL_MAX_ENTRIES.  Changing that one will break binary
 * compatibility with pre-8.0 userland and change on-disk ACL layout.
 */
pub const ACL_MAX_ENTRIES: usize = 254;

pub const POSIX1E_ACL_ACCESS_EXTATTR_NAMESPACE: i32 = EXTATTR_NAMESPACE_SYSTEM;
pub const POSIX1E_ACL_ACCESS_EXTATTR_NAME: &str = "posix1e.acl_access";
pub const POSIX1E_ACL_DEFAULT_EXTATTR_NAMESPACE: i32 = EXTATTR_NAMESPACE_SYSTEM;
pub const POSIX1E_ACL_DEFAULT_EXTATTR_NAME: &str = "posix1e.acl_default";
pub const NFS4_ACL_EXTATTR_NAMESPACE: i32 = EXTATTR_NAMESPACE_SYSTEM;
pub const NFS4_ACL_EXTATTR_NAME: &str = "nfs4.acl";
pub const OLDACL_MAX_ENTRIES: i32 = 32;

/// Current "struct acl".
#[derive(Clone)]
pub struct acl_entry_t {
    pub ae_tag: acl_tag_t,
    pub ae_id: uid_t,
    pub ae_perm: acl_perm_t,

    /// NFSv4 entry type, "allow" or "deny".  Unused in POSIX.1e ACLs.
    pub ae_entry_type: *mut acl_entry_type_t,

    /// NFSv4 ACL inheritance.  Unused in POSIX.1e ACLs.
    pub ae_flags: acl_flag_t,
}

/// Internal ACL structure, used in libc, kernel APIs and for on-disk
/// storage of NFSv4 ACLs.  POSIX.1e ACLs use "struct oldacl" for on-disk
/// storage.
#[derive(Clone)]
pub struct acl_t {
    pub acl_maxcnt: u32,
    pub acl_cnt: u32,

    // Will be required e.g. to implement NFSv4.1 ACL inheritance.
    acl_spare: [i32; 4],

    pub acl_entry: [acl_entry_t; ACL_MAX_ENTRIES],
}

/// ACL structure internal to libc.
#[derive(Clone)]
pub struct acl_t_struct_t {
    pub ats_acl: *mut acl_t,
    pub ats_cur_entry: i32,

    /// ats_brand is for libc internal bookkeeping only.
    /// Applications should use acl_get_brand_np(3).
    /// Kernel code should use the "type" argument passed
    /// to VOP_SETACL, VOP_GETACL or VOP_ACLCHECK calls;
    /// ACL_TYPE_ACCESS or ACL_TYPE_DEFAULT mean POSIX.1e
    /// ACL, ACL_TYPE_NFS4 means NFSv4 ACL.
    pub ats_brand: i32,
}
//pub type acl_t = *mut acl_t_struct_t;

/// Possible valid values for ats_brand field.
pub const ACL_BRAND_UNKNOWN: i32 = 0;
pub const ACL_BRAND_POSIX: i32 = 1;
pub const ACL_BRAND_NFS4: i32 = 2;

/// Possible valid values for ae_tag field.  For explanation, see acl(9).
pub const ACL_UNDEFINED_TAG: i32 = 0x00000000;
pub const ACL_USER_OBJ: i32 = 0x00000001;
pub const ACL_USER: i32 = 0x00000002;
pub const ACL_GROUP_OBJ: i32 = 0x00000004;
pub const ACL_GROUP: i32 = 0x00000008;
pub const ACL_MASK: i32 = 0x00000010;
pub const ACL_OTHER: i32 = 0x00000020;
pub const ACL_OTHER_OBJ: i32 = ACL_OTHER;
pub const ACL_EVERYONE: i32 = 0x00000040;

/// Possible valid values for ae_entry_type field, valid only for NFSv4 ACLs.
pub const ACL_ENTRY_TYPE_ALLOW: i32 = 0x0100;
pub const ACL_ENTRY_TYPE_DENY: i32 = 0x0200;
pub const ACL_ENTRY_TYPE_AUDIT: i32 = 0x0400;
pub const ACL_ENTRY_TYPE_ALARM: i32 = 0x0800;

/// Possible valid values for acl_type_t arguments.
///
/// First two are provided only for backwards binary compatibility.
pub const ACL_TYPE_ACCESS_OLD: i32 = 0x00000000;
pub const ACL_TYPE_DEFAULT_OLD: i32 = 0x00000001;
pub const ACL_TYPE_ACCESS: i32 = 0x00000002;
pub const ACL_TYPE_DEFAULT: i32 = 0x00000003;
pub const ACL_TYPE_NFS4: i32 = 0x00000004;

/// Possible bits in ae_perm field for POSIX.1e ACLs.
///
/// Note that ACL_EXECUTE may be used in both NFSv4 and POSIX.1e ACLs.
pub const ACL_EXECUTE: i32 = 0x0001;
pub const ACL_WRITE: i32 = 0x0002;
pub const ACL_READ: i32 = 0x0004;
pub const ACL_PERM_NONE: i32 = 0x0000;
pub const ACL_PERM_BITS: i32 = ACL_EXECUTE | ACL_WRITE | ACL_READ;
pub const ACL_POSIX1E_BITS: i32 = ACL_EXECUTE | ACL_WRITE | ACL_READ;

/// Possible bits in ae_perm field for NFSv4 ACLs.
pub const ACL_READ_DATA: i32 = 0x00000008;
pub const ACL_LIST_DIRECTORY: i32 = 0x00000008;
pub const ACL_WRITE_DATA: i32 = 0x00000010;
pub const ACL_ADD_FILE: i32 = 0x00000010;
pub const ACL_APPEND_DATA: i32 = 0x00000020;
pub const ACL_ADD_SUBDIRECTORY: i32 = 0x00000020;
pub const ACL_READ_NAMED_ATTRS: i32 = 0x00000040;
pub const ACL_WRITE_NAMED_ATTRS: i32 = 0x00000080;
/// ACL_EXECUTE is defined above.
pub const ACL_DELETE_CHILD: i32 = 0x00000100;
pub const ACL_READ_ATTRIBUTES: i32 = 0x00000200;
pub const ACL_WRITE_ATTRIBUTES: i32 = 0x00000400;
pub const ACL_DELETE: i32 = 0x00000800;
pub const ACL_READ_ACL: i32 = 0x00001000;
pub const ACL_WRITE_ACL: i32 = 0x00002000;
pub const ACL_WRITE_OWNER: i32 = 0x00004000;
pub const ACL_SYNCHRONIZE: i32 = 0x00008000;

pub const ACL_FULL_SET: i32 = ACL_READ_DATA
    | ACL_WRITE_DATA
    | ACL_APPEND_DATA
    | ACL_READ_NAMED_ATTRS
    | ACL_WRITE_NAMED_ATTRS
    | ACL_EXECUTE
    | ACL_DELETE_CHILD
    | ACL_READ_ATTRIBUTES
    | ACL_WRITE_ATTRIBUTES
    | ACL_DELETE
    | ACL_READ_ACL
    | ACL_WRITE_ACL
    | ACL_WRITE_OWNER
    | ACL_SYNCHRONIZE;

pub const ACL_MODIFY_SET: i32 = ACL_FULL_SET & !(ACL_WRITE_ACL | ACL_WRITE_OWNER);

pub const ACL_READ_SET: i32 =
    ACL_READ_DATA | ACL_READ_NAMED_ATTRS | ACL_READ_ATTRIBUTES | ACL_READ_ACL;

pub const ACL_WRITE_SET: i32 =
    ACL_WRITE_DATA | ACL_APPEND_DATA | ACL_WRITE_NAMED_ATTRS | ACL_WRITE_ATTRIBUTES;

pub const ACL_NFS4_PERM_BITS: i32 = ACL_FULL_SET;

/// Possible entry_id values for acl_get_entry(3).
pub const ACL_FIRST_ENTRY: i32 = 0;
pub const ACL_NEXT_ENTRY: i32 = 1;

/// Possible values in ae_flags field; valid only for NFSv4 ACLs.
pub const ACL_ENTRY_FILE_INHERIT: i32 = 0x0001;
pub const ACL_ENTRY_DIRECTORY_INHERIT: i32 = 0x0002;
pub const ACL_ENTRY_NO_PROPAGATE_INHERIT: i32 = 0x0004;
pub const ACL_ENTRY_INHERIT_ONLY: i32 = 0x0008;
pub const ACL_ENTRY_SUCCESSFUL_ACCESS: i32 = 0x0010;
pub const ACL_ENTRY_FAILED_ACCESS: i32 = 0x0020;
pub const ACL_ENTRY_INHERITED: i32 = 0x0080;

pub const ACL_FLAGS_BITS: i32 = ACL_ENTRY_FILE_INHERIT
    | ACL_ENTRY_DIRECTORY_INHERIT
    | ACL_ENTRY_NO_PROPAGATE_INHERIT
    | ACL_ENTRY_INHERIT_ONLY
    | ACL_ENTRY_SUCCESSFUL_ACCESS
    | ACL_ENTRY_FAILED_ACCESS
    | ACL_ENTRY_INHERITED;

/// Undefined value in ae_id field.
/// ae_id should be set to this value if ae_tag is ACL_USER_OBJ, ACL_GROUP_OBJ,
/// ACL_OTHER or ACL_EVERYONE.
pub const ACL_UNDEFINED_ID: uid_t = -1_i32 as uid_t;

/// Possible values for _flags parameter in acl_to_text_np(3).
pub const ACL_TEXT_VERBOSE: i32 = 0x01;
pub const ACL_TEXT_NUMERIC_IDS: i32 = 0x02;
pub const ACL_TEXT_APPEND_ID: i32 = 0x04;

/*
 * POSIX.1e ACLs are capable of expressing the read, write, and execute bits
 * of the POSIX mode field.  We provide two masks: one that defines the bits
 * the ACL will replace in the mode, and the other that defines the bits that
 * must be preseved when an ACL is updating a mode.
 */
pub const ACL_OVERRIDE_MASK: i32 = S_IRWXU | S_IRWXG | S_IRWXO;
pub const ACL_PRESERVE_MASK: i32 = !ACL_OVERRIDE_MASK;
