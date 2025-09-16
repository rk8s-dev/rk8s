// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `linux/key.h`

#![allow(clippy::module_name_repetitions)]

/// key handle serial number
pub type key_serial_t = i32;

/// key handle permissions mask
pub type key_perm_t = u32;

/// possessor can view a key's attributes
pub const KEY_POS_VIEW: i32 = 0x0100_0000;
/// possessor can read key payload / view keyring
pub const KEY_POS_READ: i32 = 0x0200_0000;
/// possessor can update key payload / add link to keyring
pub const KEY_POS_WRITE: i32 = 0x0400_0000;
/// possessor can find a key in search / search a keyring
pub const KEY_POS_SEARCH: i32 = 0x0800_0000;
/// possessor can create a link to a key/keyring
pub const KEY_POS_LINK: i32 = 0x1000_0000;
/// possessor can set key attributes
pub const KEY_POS_SETATTR: i32 = 0x2000_0000;
pub const KEY_POS_ALL: i32 = 0x3f00_0000;

/// user permissions...
pub const KEY_USR_VIEW: i32 = 0x0001_0000;
pub const KEY_USR_READ: i32 = 0x0002_0000;
pub const KEY_USR_WRITE: i32 = 0x0004_0000;
pub const KEY_USR_SEARCH: i32 = 0x0008_0000;
pub const KEY_USR_LINK: i32 = 0x0010_0000;
pub const KEY_USR_SETATTR: i32 = 0x0020_0000;
pub const KEY_USR_ALL: i32 = 0x003f_0000;

/// group permissions...
pub const KEY_GRP_VIEW: i32 = 0x0000_0100;
pub const KEY_GRP_READ: i32 = 0x0000_0200;
pub const KEY_GRP_WRITE: i32 = 0x0000_0400;
pub const KEY_GRP_SEARCH: i32 = 0x0000_0800;
pub const KEY_GRP_LINK: i32 = 0x0000_1000;
pub const KEY_GRP_SETATTR: i32 = 0x0000_2000;
pub const KEY_GRP_ALL: i32 = 0x0000_3f00;

/// third party permissions...
pub const KEY_OTH_VIEW: i32 = 0x0000_0001;
pub const KEY_OTH_READ: i32 = 0x0000_0002;
pub const KEY_OTH_WRITE: i32 = 0x0000_0004;
pub const KEY_OTH_SEARCH: i32 = 0x0000_0008;
pub const KEY_OTH_LINK: i32 = 0x0000_0010;
pub const KEY_OTH_SETATTR: i32 = 0x0000_0020;
pub const KEY_OTH_ALL: i32 = 0x0000_003f;

#[allow(overflowing_literals)]
pub const KEY_PERM_UNDEF: i32 = 0xffff_ffff;

//struct keyring_index_key {
//	struct key_type		*type;
//	const char		*description;
//	size_t			desc_len;
//};
//
//union key_payload {
//	void __rcu		*rcu_data0;
//	void			*data[4];
//};

/*
 * key reference with possession attribute handling
 *
 * NOTE! key_ref_t is a typedef'd pointer to a type that is not actually
 * defined. This is because we abuse the bottom bit of the reference to carry a
 * flag to indicate whether the calling process possesses that key in one of
 * its keyrings.
 *
 * the key_ref_t has been made a separate type so that the compiler can reject
 * attempts to dereference it without proper conversion.
 *
 * the three functions are used to assemble and disassemble references
 */
//typedef struct __key_reference_with_attributes *key_ref_t;

//struct key_restriction {
//	key_restrict_link_func_t check;
//	struct key *key;
//	struct key_type *keytype;
//};

pub const KEY_IS_UNINSTANTIATED: i32 = 0;
/// Positively instantiated
pub const KEY_IS_POSITIVE: i32 = 1;

/// authentication token / access credential / keyring
/// - types of key include:
/// - keyrings
/// - disk encryption IDs
/// - Kerberos TGTs and tickets
pub const KEY_DEBUG_MAGIC: u32 = 0x1827_3645;
/// set if key type has been deleted
pub const KEY_FLAG_DEAD: i32 = 0;
/// set if key had been revoked
pub const KEY_FLAG_REVOKED: i32 = 1;
/// set if key consumes quota
pub const KEY_FLAG_IN_QUOTA: i32 = 2;
/// set if key is being constructed in userspace
pub const KEY_FLAG_USER_CONSTRUCT: i32 = 3;
/// set if key can be cleared by root without permission
pub const KEY_FLAG_ROOT_CAN_CLEAR: i32 = 4;
/// set if key has been invalidated
pub const KEY_FLAG_INVALIDATED: i32 = 5;
/// set if key is built in to the kernel
pub const KEY_FLAG_BUILTIN: i32 = 6;
/// set if key can be invalidated by root without permission
pub const KEY_FLAG_ROOT_CAN_INVAL: i32 = 7;
/// set if key should not be removed
pub const KEY_FLAG_KEEP: i32 = 8;
/// set if key is a user or user session keyring
pub const KEY_FLAG_UID_KEYRING: i32 = 9;

/// add to quota, reject if would overrun
pub const KEY_ALLOC_IN_QUOTA: i32 = 0x0000;
/// add to quota, permit even if overrun
pub const KEY_ALLOC_QUOTA_OVERRUN: i32 = 0x0001;
/// not in quota
pub const KEY_ALLOC_NOT_IN_QUOTA: i32 = 0x0002;
/// Key is built into kernel
pub const KEY_ALLOC_BUILT_IN: i32 = 0x0004;
/// Override the check on restricted keyrings
pub const KEY_ALLOC_BYPASS_RESTRICTION: i32 = 0x0008;
/// allocating a user or user session keyring
pub const KEY_ALLOC_UID_KEYRING: i32 = 0x0010;

/// The permissions required on a key that we're looking up.
/// Require permission to view attributes
pub const KEY_NEED_VIEW: i32 = 0x01;
/// Require permission to read content
pub const KEY_NEED_READ: i32 = 0x02;
/// Require permission to update / modify
pub const KEY_NEED_WRITE: i32 = 0x04;
/// Require permission to search (keyring) or find (key)
pub const KEY_NEED_SEARCH: i32 = 0x08;
/// Require permission to link
pub const KEY_NEED_LINK: i32 = 0x10;
/// Require permission to change attributes
pub const KEY_NEED_SETATTR: i32 = 0x20;
/// All the above permissions
pub const KEY_NEED_ALL: i32 = 0x3f;
