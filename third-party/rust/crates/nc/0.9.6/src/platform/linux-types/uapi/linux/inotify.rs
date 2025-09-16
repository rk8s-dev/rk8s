// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/inotify.h`

#![allow(clippy::module_name_repetitions)]

use crate::{O_CLOEXEC, O_NONBLOCK};

/// struct `inotify_event` - structure read from the inotify device for each event
///
/// When you are watching a directory, you will receive the filename for events
/// such as `IN_CREATE`, `IN_DELETE`, `IN_OPEN`, `IN_CLOSE`, ...,
/// relative to the wd.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct inotify_event_t {
    /// watch descriptor
    pub wd: u32,

    /// watch mask
    pub mask: u32,

    /// cookie to synchronize two events
    pub cookie: u32,

    /// length (including nulls) of name
    pub len: u32,

    /// stub for possible name
    pub name: usize,
}

/// the following are legal, implemented events that user-space can watch for
/// File was accessed
pub const IN_ACCESS: u32 = 0x0000_0001;

/// File was modified
pub const IN_MODIFY: u32 = 0x0000_0002;

/// Metadata changed
pub const IN_ATTRIB: u32 = 0x0000_0004;

/// Writtable file was closed
pub const IN_CLOSE_WRITE: u32 = 0x0000_0008;

/// Unwrittable file closed
pub const IN_CLOSE_NOWRITE: u32 = 0x0000_0010;

/// File was opened
pub const IN_OPEN: u32 = 0x0000_0020;

/// File was moved from X
pub const IN_MOVED_FROM: u32 = 0x0000_0040;

/// File was moved to Y
pub const IN_MOVED_TO: u32 = 0x0000_0080;

/// Subfile was created
pub const IN_CREATE: u32 = 0x0000_0100;

/// Subfile was deleted
pub const IN_DELETE: u32 = 0x0000_0200;

/// Self was deleted
pub const IN_DELETE_SELF: u32 = 0x0000_0400;

/// Self was moved
pub const IN_MOVE_SELF: u32 = 0x0000_0800;

/// the following are legal events.  they are sent as needed to any watch
/// Backing fs was unmounted
pub const IN_UNMOUNT: u32 = 0x0000_2000;

/// Event queued overflowed
pub const IN_Q_OVERFLOW: u32 = 0x0000_4000;

/// File was ignored
pub const IN_IGNORED: u32 = 0x0000_8000;

// helper events
/// close
pub const IN_CLOSE: u32 = IN_CLOSE_WRITE | IN_CLOSE_NOWRITE;

/// moves
pub const IN_MOVE: u32 = IN_MOVED_FROM | IN_MOVED_TO;

/// special flags
/// only watch the path if it is a directory
pub const IN_ONLYDIR: u32 = 0x0100_0000;

/// don't follow a sym link
pub const IN_DONT_FOLLOW: u32 = 0x0200_0000;

/// exclude events on unlinked objects
pub const IN_EXCL_UNLINK: u32 = 0x0400_0000;

/// only create watches
pub const IN_MASK_CREATE: u32 = 0x1000_0000;

/// add to the mask of an already existing watch
pub const IN_MASK_ADD: u32 = 0x2000_0000;

/// event occurred against dir
pub const IN_ISDIR: u32 = 0x4000_0000;

#[allow(overflowing_literals)]
/// only send event once
pub const IN_ONESHOT: u32 = 0x8000_0000;

/// All of the events - we build the list by hand so that we can add flags in
/// the future and not break backward compatibility.
///
/// Apps will get only the events that they originally wanted.
/// Be sure to add new events here!
pub const IN_ALL_EVENTS: u32 = IN_ACCESS
    | IN_MODIFY
    | IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_CLOSE_NOWRITE
    | IN_OPEN
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_DELETE
    | IN_CREATE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;

/// Flags for `sys_inotify_init1`.
pub const IN_CLOEXEC: i32 = O_CLOEXEC;
pub const IN_NONBLOCK: i32 = O_NONBLOCK;
