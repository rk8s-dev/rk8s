// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/blktrace_api.h`

/// Trace categories
pub type blktrace_cat_t = u32;

/// reads
pub const BLK_TC_READ: u32 = 1 << 0;
/// writes
pub const BLK_TC_WRITE: u32 = 1 << 1;
/// flush
pub const BLK_TC_FLUSH: u32 = 1 << 2;
/// sync IO
pub const BLK_TC_SYNC: u32 = 1 << 3;
pub const BLK_TC_SYNCIO: u32 = BLK_TC_SYNC;
/// queueing/merging
pub const BLK_TC_QUEUE: u32 = 1 << 4;
/// requeueing
pub const BLK_TC_REQUEUE: u32 = 1 << 5;
/// issue
pub const BLK_TC_ISSUE: u32 = 1 << 6;
/// completions
pub const BLK_TC_COMPLETE: u32 = 1 << 7;
/// fs requests
pub const BLK_TC_FS: u32 = 1 << 8;
/// pc requests
pub const BLK_TC_PC: u32 = 1 << 9;
/// special message
pub const BLK_TC_NOTIFY: u32 = 1 << 10;
/// readahead
pub const BLK_TC_AHEAD: u32 = 1 << 11;
/// metadata
pub const BLK_TC_META: u32 = 1 << 12;
/// discard requests
pub const BLK_TC_DISCARD: u32 = 1 << 13;
/// binary per-driver data
pub const BLK_TC_DRV_DATA: u32 = 1 << 14;
/// fua requests
pub const BLK_TC_FUA: u32 = 1 << 15;
// we've run out of bits!
pub const BLK_TC_END: u32 = 1 << 15;

pub const BLK_TC_SHIFT: u32 = 16;
#[must_use]
pub const fn BLK_TC_ACT(act: u32) -> u32 {
    act << BLK_TC_SHIFT
}

/// Basic trace actions
pub type blktrace_act_t = u32;

/// queued
pub const __BLK_TA_QUEUE: blktrace_act_t = 1;

/// back merged to existing rq
pub const __BLK_TA_BACKMERGE: blktrace_act_t = 2;

/// front merge to existing rq
pub const __BLK_TA_FRONTMERGE: blktrace_act_t = 3;

/// allocated new request
pub const __BLK_TA_GETRQ: blktrace_act_t = 4;

/// sleeping on rq allocation
pub const __BLK_TA_SLEEPRQ: blktrace_act_t = 5;

/// request requeued
pub const __BLK_TA_REQUEUE: blktrace_act_t = 6;

/// sent to driver
pub const __BLK_TA_ISSUE: blktrace_act_t = 7;

/// completed by driver
pub const __BLK_TA_COMPLETE: blktrace_act_t = 8;

/// queue was plugged
pub const __BLK_TA_PLUG: blktrace_act_t = 9;

/// queue was unplugged by io
pub const __BLK_TA_UNPLUG_IO: blktrace_act_t = 10;

/// queue was unplugged by timer
pub const __BLK_TA_UNPLUG_TIMER: blktrace_act_t = 11;

/// insert request
pub const __BLK_TA_INSERT: blktrace_act_t = 12;

/// bio was split
pub const __BLK_TA_SPLIT: blktrace_act_t = 13;

/// bio was bounced
pub const __BLK_TA_BOUNCE: blktrace_act_t = 14;

/// bio was remapped
pub const __BLK_TA_REMAP: blktrace_act_t = 15;

/// request aborted
pub const __BLK_TA_ABORT: blktrace_act_t = 16;

/// driver-specific binary data
pub const __BLK_TA_DRV_DATA: blktrace_act_t = 17;

/// from a cgroup
pub const __BLK_TA_CGROUP: blktrace_act_t = 1 << 8;

/// Notify events.
pub type blktrace_notify_t = u32;

/// establish pid/name mapping
pub const __BLK_TN_PROCESS: blktrace_notify_t = 0;
/// include system clock
pub const __BLK_TN_TIMESTAMP: blktrace_notify_t = 1;
/// Character string message
pub const __BLK_TN_MESSAGE: blktrace_notify_t = 2;
/// from a cgroup
pub const __BLK_TN_CGROUP: blktrace_notify_t = __BLK_TA_CGROUP;

/// Trace actions in full.
///
/// Additionally, read or write is masked
pub const BLK_TA_QUEUE: u32 = __BLK_TA_QUEUE | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_BACKMERGE: u32 = __BLK_TA_BACKMERGE | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_FRONTMERGE: u32 = __BLK_TA_FRONTMERGE | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_GETRQ: u32 = __BLK_TA_GETRQ | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_SLEEPRQ: u32 = __BLK_TA_SLEEPRQ | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_REQUEUE: u32 = __BLK_TA_REQUEUE | BLK_TC_ACT(BLK_TC_REQUEUE);
pub const BLK_TA_ISSUE: u32 = __BLK_TA_ISSUE | BLK_TC_ACT(BLK_TC_ISSUE);
pub const BLK_TA_COMPLETE: u32 = __BLK_TA_COMPLETE | BLK_TC_ACT(BLK_TC_COMPLETE);
pub const BLK_TA_PLUG: u32 = __BLK_TA_PLUG | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_UNPLUG_IO: u32 = __BLK_TA_UNPLUG_IO | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_UNPLUG_TIMER: u32 = __BLK_TA_UNPLUG_TIMER | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_INSERT: u32 = __BLK_TA_INSERT | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_SPLIT: u32 = __BLK_TA_SPLIT;
pub const BLK_TA_BOUNCE: u32 = __BLK_TA_BOUNCE;
pub const BLK_TA_REMAP: u32 = __BLK_TA_REMAP | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_ABORT: u32 = __BLK_TA_ABORT | BLK_TC_ACT(BLK_TC_QUEUE);
pub const BLK_TA_DRV_DATA: u32 = __BLK_TA_DRV_DATA | BLK_TC_ACT(BLK_TC_DRV_DATA);

pub const BLK_TN_PROCESS: u32 = __BLK_TN_PROCESS | BLK_TC_ACT(BLK_TC_NOTIFY);
pub const BLK_TN_TIMESTAMP: u32 = __BLK_TN_TIMESTAMP | BLK_TC_ACT(BLK_TC_NOTIFY);
pub const BLK_TN_MESSAGE: u32 = __BLK_TN_MESSAGE | BLK_TC_ACT(BLK_TC_NOTIFY);

pub const BLK_IO_TRACE_MAGIC: i32 = 0x6561_7400;
pub const BLK_IO_TRACE_VERSION: i32 = 0x07;

/// The trace itself
pub struct blk_io_trace_t {
    /// MAGIC << 8 | version
    pub magic: u32,

    /// event number
    pub sequence: u32,

    /// in nanoseconds
    pub time: u64,

    /// disk offset
    pub sector: u64,

    /// transfer length
    pub bytes: u32,

    /// what happened
    pub action: u32,

    /// who did it
    pub pid: u32,

    /// device number
    pub device: u32,

    /// on what cpu did it happen
    pub cpu: u32,

    /// completion error
    pub error: u16,

    /// length of data after this trace
    /// cgroup id will be stored here if exists
    pub pdu_len: u16,
}

/// The remap event
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct blk_io_trace_remap_t {
    // TODO(Shaohua): Convert to __be32
    pub device_from: i32,
    pub device_to: i32,
    pub sector_from: i64,
}

#[allow(non_upper_case_globals)]
pub const Blktrace_setup: i32 = 1;
#[allow(non_upper_case_globals)]
pub const Blktrace_running: i32 = 2;
#[allow(non_upper_case_globals)]
pub const Blktrace_stopped: i32 = 3;

pub const BLKTRACE_BDEV_SIZE: usize = 32;

/// User setup structure passed with BLKTRACESETUP
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct blk_user_trace_setup_t {
    /// output
    pub name: [u8; BLKTRACE_BDEV_SIZE],

    /// input
    pub act_mask: u16,

    /// input
    pub buf_size: u32,

    /// input
    pub buf_nr: u32,

    pub start_lba: u64,

    pub end_bla: u64,

    pub pid: u32,
}
