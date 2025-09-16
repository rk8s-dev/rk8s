// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by APache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/mqueue.h`

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct mq_attr_t {
    /// Message queue flags.
    pub mq_flags: isize,
    /// Maximum number of messages.
    pub mq_maxmsg: isize,
    /// Maximum message size.
    pub mq_msgsize: isize,
    /// Number of messages currently queued.
    pub mq_curmsgs: isize,

    // Ignored for input, zeroed for output
    __reserved: [isize; 4],
}
