// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/syslog.h`

/// Close the log.  Currently a NOP.
pub const SYSLOG_ACTION_CLOSE: i32 = 0;
/// Open the log. Currently a NOP.
pub const SYSLOG_ACTION_OPEN: i32 = 1;
/// Read from the log.
pub const SYSLOG_ACTION_READ: i32 = 2;
/// Read all messages remaining in the ring buffer.
pub const SYSLOG_ACTION_READ_ALL: i32 = 3;
/// Read and clear all messages remaining in the ring buffer
pub const SYSLOG_ACTION_READ_CLEAR: i32 = 4;
/// Clear ring buffer.
pub const SYSLOG_ACTION_CLEAR: i32 = 5;
/// Disable printk's to console
pub const SYSLOG_ACTION_CONSOLE_OFF: i32 = 6;
/// Enable printk's to console
pub const SYSLOG_ACTION_CONSOLE_ON: i32 = 7;
/// Set level of messages printed to console
pub const SYSLOG_ACTION_CONSOLE_LEVEL: i32 = 8;
/// Return number of unread characters in the log buffer
pub const SYSLOG_ACTION_SIZE_UNREAD: i32 = 9;
/// Return size of the log buffer
pub const SYSLOG_ACTION_SIZE_BUFFER: i32 = 10;

pub const SYSLOG_FROM_READER: i32 = 0;
pub const SYSLOG_FROM_PROC: i32 = 1;
