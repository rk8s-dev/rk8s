// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/mqueue.h`

/// Maximal number of mqueue descriptors, that process could open
pub const MQ_OPEN_MAX: i32 = 512;

/// Maximal priority of the message
pub const MQ_PRIO_MAX: i32 = 32;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct mq_attr_t {
    /// Flags of message queue
    pub mq_flags: isize,
    /// Maximum number of messages
    pub mq_maxmsg: isize,
    /// Maximum size of the message
    pub mq_msgsize: isize,
    /// Count of the queued messages
    pub mq_curmsgs: isize,
}
