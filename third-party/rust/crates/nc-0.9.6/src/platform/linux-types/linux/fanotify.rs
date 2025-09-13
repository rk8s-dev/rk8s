// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/fanotify.h`

use crate::{
    FAN_ACCESS, FAN_ACCESS_PERM, FAN_ALLOW, FAN_ATTRIB, FAN_AUDIT, FAN_CLASS_CONTENT,
    FAN_CLASS_NOTIF, FAN_CLASS_PRE_CONTENT, FAN_CLOEXEC, FAN_CLOSE, FAN_CREATE, FAN_DELETE,
    FAN_DELETE_SELF, FAN_DENY, FAN_EVENT_ON_CHILD, FAN_FS_ERROR, FAN_INFO, FAN_MARK_ADD,
    FAN_MARK_DONT_FOLLOW, FAN_MARK_EVICTABLE, FAN_MARK_FILESYSTEM, FAN_MARK_FLUSH, FAN_MARK_IGNORE,
    FAN_MARK_IGNORED_MASK, FAN_MARK_IGNORED_SURV_MODIFY, FAN_MARK_INODE, FAN_MARK_MOUNT,
    FAN_MARK_ONLYDIR, FAN_MARK_REMOVE, FAN_MODIFY, FAN_MOVE, FAN_MOVE_SELF, FAN_NONBLOCK,
    FAN_ONDIR, FAN_OPEN, FAN_OPEN_EXEC, FAN_OPEN_EXEC_PERM, FAN_OPEN_PERM, FAN_Q_OVERFLOW,
    FAN_RENAME, FAN_REPORT_DFID_NAME_TARGET, FAN_REPORT_PIDFD, FAN_REPORT_TID, FAN_UNLIMITED_MARKS,
    FAN_UNLIMITED_QUEUE,
};

/// Flags allowed to be passed from/to userspace.
///
/// We intentionally do not add new bits to the old `FAN_ALL`_* constants, because
/// they are uapi exposed constants. If there are programs out there using
/// these constant, the programs may break if re-compiled with new uapi headers
/// and then run on an old kernel.
/// Group classes where permission events are allowed
pub const FANOTIFY_PERM_CLASSES: u32 = FAN_CLASS_CONTENT | FAN_CLASS_PRE_CONTENT;

pub const FANOTIFY_CLASS_BITS: u32 = FAN_CLASS_NOTIF | FANOTIFY_PERM_CLASSES;

pub const FANOTIFY_FID_BITS: u32 = FAN_REPORT_DFID_NAME_TARGET;

pub const FANOTIFY_INFO_MODES: u32 = FANOTIFY_FID_BITS | FAN_REPORT_PIDFD;

/// `fanotify_init()` flags that require `CAP_SYS_ADMIN`.
///
/// We do not allow unprivileged groups to request permission events.
/// We do not allow unprivileged groups to get other process pid in events.
/// We do not allow unprivileged groups to use unlimited resources.
pub const FANOTIFY_ADMIN_INIT_FLAGS: u32 = FANOTIFY_PERM_CLASSES
    | FAN_REPORT_TID
    | FAN_REPORT_PIDFD
    | FAN_UNLIMITED_QUEUE
    | FAN_UNLIMITED_MARKS;

/// `fanotify_init()` flags that are allowed for user without `CAP_SYS_ADMIN`.
///
/// `FAN_CLASS_NOTIF` is the only class we allow for unprivileged group.
/// We do not allow unprivileged groups to get file descriptors in events,
/// so one of the flags for reporting file handles is required.
pub const FANOTIFY_USER_INIT_FLAGS: u32 =
    FAN_CLASS_NOTIF | FANOTIFY_FID_BITS | FAN_CLOEXEC | FAN_NONBLOCK;

pub const FANOTIFY_INIT_FLAGS: u32 = FANOTIFY_ADMIN_INIT_FLAGS | FANOTIFY_USER_INIT_FLAGS;

/// Internal group flags
pub const FANOTIFY_UNPRIV: u32 = 0x8000_0000;
pub const FANOTIFY_INTERNAL_GROUP_FLAGS: u32 = FANOTIFY_UNPRIV;

pub const FANOTIFY_MARK_TYPE_BITS: u32 = FAN_MARK_INODE | FAN_MARK_MOUNT | FAN_MARK_FILESYSTEM;

pub const FANOTIFY_MARK_CMD_BITS: u32 = FAN_MARK_ADD | FAN_MARK_REMOVE | FAN_MARK_FLUSH;

pub const FANOTIFY_MARK_IGNORE_BITS: u32 = FAN_MARK_IGNORED_MASK | FAN_MARK_IGNORE;

pub const FANOTIFY_MARK_FLAGS: u32 = FANOTIFY_MARK_TYPE_BITS
    | FANOTIFY_MARK_CMD_BITS
    | FANOTIFY_MARK_IGNORE_BITS
    | FAN_MARK_DONT_FOLLOW
    | FAN_MARK_ONLYDIR
    | FAN_MARK_IGNORED_SURV_MODIFY
    | FAN_MARK_EVICTABLE;

/// Events that can be reported with data type `FSNOTIFY_EVENT_PATH`.
/// Note that `FAN_MODIFY` can also be reported with data type
/// `FSNOTIFY_EVENT_INODE`.
pub const FANOTIFY_PATH_EVENTS: u32 =
    FAN_ACCESS | FAN_MODIFY | FAN_CLOSE | FAN_OPEN | FAN_OPEN_EXEC;

/// Directory entry modification events - reported only to directory
/// where entry is modified and not to a watching parent.
pub const FANOTIFY_DIRENT_EVENTS: u32 = FAN_MOVE | FAN_CREATE | FAN_DELETE | FAN_RENAME;

/// Events that can be reported with event->fd
pub const FANOTIFY_FD_EVENTS: u32 = FANOTIFY_PATH_EVENTS | FANOTIFY_PERM_EVENTS;

/// Events that can only be reported with data type `FSNOTIFY_EVENT_INODE`
pub const FANOTIFY_INODE_EVENTS: u32 =
    FANOTIFY_DIRENT_EVENTS | FAN_ATTRIB | FAN_MOVE_SELF | FAN_DELETE_SELF;

/// Events that can only be reported with data type `FSNOTIFY_EVENT_ERROR`
pub const FANOTIFY_ERROR_EVENTS: u32 = FAN_FS_ERROR;

/// Events that user can request to be notified on
pub const FANOTIFY_EVENTS: u32 =
    FANOTIFY_PATH_EVENTS | FANOTIFY_INODE_EVENTS | FANOTIFY_ERROR_EVENTS;

/// Events that require a permission response from user
pub const FANOTIFY_PERM_EVENTS: u32 = FAN_OPEN_PERM | FAN_ACCESS_PERM | FAN_OPEN_EXEC_PERM;

/// Extra flags that may be reported with event or control handling of events
pub const FANOTIFY_EVENT_FLAGS: u32 = FAN_EVENT_ON_CHILD | FAN_ONDIR;

/// Events that may be reported to user
pub const FANOTIFY_OUTGOING_EVENTS: u32 =
    FANOTIFY_EVENTS | FANOTIFY_PERM_EVENTS | FAN_Q_OVERFLOW | FAN_ONDIR;

/// Events and flags relevant only for directories
pub const FANOTIFY_DIRONLY_EVENT_BITS: u32 =
    FANOTIFY_DIRENT_EVENTS | FAN_EVENT_ON_CHILD | FAN_ONDIR;

pub const ALL_FANOTIFY_EVENT_BITS: u32 = FANOTIFY_OUTGOING_EVENTS | FANOTIFY_EVENT_FLAGS;

/// These masks check for invalid bits in permission responses.
pub const FANOTIFY_RESPONSE_ACCESS: u32 = FAN_ALLOW | FAN_DENY;
pub const FANOTIFY_RESPONSE_FLAGS: u32 = FAN_AUDIT | FAN_INFO;
pub const FANOTIFY_RESPONSE_VALID_MASK: u32 = FANOTIFY_RESPONSE_ACCESS | FANOTIFY_RESPONSE_FLAGS;
