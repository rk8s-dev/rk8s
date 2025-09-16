// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/fanotify.h`

/// the following events that user-space can register for
/// File was accessed
pub const FAN_ACCESS: u32 = 0x0000_0001;
/// File was modified
pub const FAN_MODIFY: u32 = 0x0000_0002;
/// Metadata changed
pub const FAN_ATTRIB: u32 = 0x0000_0004;
/// Writable file closed
pub const FAN_CLOSE_WRITE: u32 = 0x0000_0008;
/// Unwritable file closed
pub const FAN_CLOSE_NOWRITE: u32 = 0x0000_0010;
/// File was opened
pub const FAN_OPEN: u32 = 0x0000_0020;
/// File was moved from X
pub const FAN_MOVED_FROM: u32 = 0x0000_0040;
/// File was moved to Y
pub const FAN_MOVED_TO: u32 = 0x0000_0080;
/// Subfile was created
pub const FAN_CREATE: u32 = 0x0000_0100;
/// Subfile was deleted
pub const FAN_DELETE: u32 = 0x0000_0200;
/// Self was deleted
pub const FAN_DELETE_SELF: u32 = 0x0000_0400;
/// Self was moved
pub const FAN_MOVE_SELF: u32 = 0x0000_0800;
/// File was opened for exec
pub const FAN_OPEN_EXEC: u32 = 0x0000_1000;

/// Event queued overflowed
pub const FAN_Q_OVERFLOW: u32 = 0x0000_4000;
/// Filesystem error
pub const FAN_FS_ERROR: u32 = 0x0000_8000;

/// File open in perm check
pub const FAN_OPEN_PERM: u32 = 0x0001_0000;
/// File accessed in perm check
pub const FAN_ACCESS_PERM: u32 = 0x0002_0000;
/// File open/exec in perm check
pub const FAN_OPEN_EXEC_PERM: u32 = 0x0004_0000;

/// Interested in child events
pub const FAN_EVENT_ON_CHILD: u32 = 0x0800_0000;

/// File was renamed
pub const FAN_RENAME: u32 = 0x1000_0000;

/// Event occurred against dir
pub const FAN_ONDIR: u32 = 0x4000_0000;

/// helper events
/// close
pub const FAN_CLOSE: u32 = FAN_CLOSE_WRITE | FAN_CLOSE_NOWRITE;
/// moves
pub const FAN_MOVE: u32 = FAN_MOVED_FROM | FAN_MOVED_TO;

/// flags used for `fanotify_init()`
pub const FAN_CLOEXEC: u32 = 0x0000_0001;
pub const FAN_NONBLOCK: u32 = 0x0000_0002;

/// These are NOT bitwise flags.  Both bits are used together.
pub const FAN_CLASS_NOTIF: u32 = 0x0000_0000;
pub const FAN_CLASS_CONTENT: u32 = 0x0000_0004;
pub const FAN_CLASS_PRE_CONTENT: u32 = 0x0000_0008;

/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_CLASS_BITS: u32 = FAN_CLASS_NOTIF | FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT;

pub const FAN_UNLIMITED_QUEUE: u32 = 0x0000_0010;
pub const FAN_UNLIMITED_MARKS: u32 = 0x0000_0020;
pub const FAN_ENABLE_AUDIT: u32 = 0x0000_0040;

/// Flags to determine fanotify event format
/// Report pidfd for event->pid
pub const FAN_REPORT_PIDFD: u32 = 0x0000_0080;
/// event->pid is thread id
pub const FAN_REPORT_TID: u32 = 0x0000_0100;
/// Report unique file id
pub const FAN_REPORT_FID: u32 = 0x0000_0200;
/// Report unique directory id
pub const FAN_REPORT_DIR_FID: u32 = 0x0000_0400;
/// Report events with name
pub const FAN_REPORT_NAME: u32 = 0x0000_0800;
/// Report dirent target id
pub const FAN_REPORT_TARGET_FID: u32 = 0x0000_1000;

/// Convenience macro - `FAN_REPORT_NAME` requires `FAN_REPORT_DIR_FID`
pub const FAN_REPORT_DFID_NAME: u32 = FAN_REPORT_DIR_FID | FAN_REPORT_NAME;
/// Convenience macro - `FAN_REPORT_TARGET_FID` requires all other FID flags
pub const FAN_REPORT_DFID_NAME_TARGET: u32 =
    FAN_REPORT_DFID_NAME | FAN_REPORT_FID | FAN_REPORT_TARGET_FID;

/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_INIT_FLAGS: u32 =
    FAN_CLOEXEC | FAN_NONBLOCK | FAN_ALL_CLASS_BITS | FAN_UNLIMITED_QUEUE | FAN_UNLIMITED_MARKS;

/// flags used for `fanotify_modify_mark()`
pub const FAN_MARK_ADD: u32 = 0x0000_0001;
pub const FAN_MARK_REMOVE: u32 = 0x0000_0002;
pub const FAN_MARK_DONT_FOLLOW: u32 = 0x0000_0004;
pub const FAN_MARK_ONLYDIR: u32 = 0x0000_0008;
/// `FAN_MARK_MOUNT` is `0x0000_0010`
pub const FAN_MARK_IGNORED_MASK: u32 = 0x0000_0020;
pub const FAN_MARK_IGNORED_SURV_MODIFY: u32 = 0x0000_0040;
pub const FAN_MARK_FLUSH: u32 = 0x0000_0080;
/// `FAN_MARK_FILESYSTEM` is `0x0000_0100`
pub const FAN_MARK_EVICTABLE: u32 = 0x0000_0200;
/// This bit is mutually exclusive with `FAN_MARK_IGNORED_MASK` bit
pub const FAN_MARK_IGNORE: u32 = 0x0000_0400;

/// These are NOT bitwise flags.  Both bits can be used togther.
pub const FAN_MARK_INODE: u32 = 0x0000_0000;
pub const FAN_MARK_MOUNT: u32 = 0x0000_0010;
pub const FAN_MARK_FILESYSTEM: u32 = 0x0000_0100;

/// Convenience macro - `FAN_MARK_IGNORE` requires `FAN_MARK_IGNORED_SURV_MODIFY`
/// for non-inode mark types.
pub const FAN_MARK_IGNORE_SURV: u32 = FAN_MARK_IGNORE | FAN_MARK_IGNORED_SURV_MODIFY;

/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_MARK_FLAGS: u32 = FAN_MARK_ADD
    | FAN_MARK_REMOVE
    | FAN_MARK_DONT_FOLLOW
    | FAN_MARK_ONLYDIR
    | FAN_MARK_MOUNT
    | FAN_MARK_IGNORED_MASK
    | FAN_MARK_IGNORED_SURV_MODIFY
    | FAN_MARK_FLUSH;

/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_EVENTS: u32 = FAN_ACCESS | FAN_MODIFY | FAN_CLOSE | FAN_OPEN;

/// All events which require a permission response from userspace
/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_PERM_EVENTS: u32 = FAN_OPEN_PERM | FAN_ACCESS_PERM;

/// Deprecated - do not use this in programs and do not add new flags here!
pub const FAN_ALL_OUTGOING_EVENTS: u32 = FAN_ALL_EVENTS | FAN_ALL_PERM_EVENTS | FAN_Q_OVERFLOW;

pub const FANOTIFY_METADATA_VERSION: u32 = 3;

pub const FAN_EVENT_INFO_TYPE_FID: u32 = 1;
pub const FAN_EVENT_INFO_TYPE_DFID_NAME: u32 = 2;
pub const FAN_EVENT_INFO_TYPE_DFID: u32 = 3;
pub const FAN_EVENT_INFO_TYPE_PIDFD: u32 = 4;
pub const FAN_EVENT_INFO_TYPE_ERROR: u32 = 5;

/// Special info types for `FAN_RENAME`
pub const FAN_EVENT_INFO_TYPE_OLD_DFID_NAME: u32 = 10;
// Reserved for `FAN_EVENT_INFO_TYPE_OLD_DFID` 11
pub const FAN_EVENT_INFO_TYPE_NEW_DFID_NAME: u32 = 12;
// Reserved for `FAN_EVENT_INFO_TYPE_NEW_DFID` 13

pub const FAN_RESPONSE_INFO_NONE: u32 = 0;
pub const FAN_RESPONSE_INFO_AUDIT_RULE: u32 = 1;

/// Legit userspace responses to a _PERM event
pub const FAN_ALLOW: u32 = 0x01;
pub const FAN_DENY: u32 = 0x02;
/// Bitmask to create audit record for result
pub const FAN_AUDIT: u32 = 0x10;
/// Bitmask to indicate additional information
pub const FAN_INFO: u32 = 0x20;

/// No fd set in event
pub const FAN_NOFD: i32 = -1;
pub const FAN_NOPIDFD: i32 = FAN_NOFD;
pub const FAN_EPIDFD: i32 = -2;
