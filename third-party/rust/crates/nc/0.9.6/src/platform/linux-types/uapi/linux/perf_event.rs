// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/perf_event.h`
//!
//! User-space ABI bits:

#![allow(clippy::module_name_repetitions)]

/// attr.type
#[repr(u32)]
pub enum perf_type_id_t {
    PERF_TYPE_HARDWARE = 0,
    PERF_TYPE_SOFTWARE = 1,
    PERF_TYPE_TRACEPOINT = 2,
    PERF_TYPE_HW_CACHE = 3,
    PERF_TYPE_RAW = 4,
    PERF_TYPE_BREAKPOINT = 5,

    /// non-ABI
    PERF_TYPE_MAX = 6,
}

/// Generalized performance event `event_id` types, used by the
/// `attr.event_id` parameter of the `sys_perf_event_open()`
/// syscall:
///
/// Common hardware events, generalized by the kernel:
#[repr(u32)]
pub enum perf_hw_id_t {
    PERF_COUNT_HW_CPU_CYCLES = 0,
    PERF_COUNT_HW_INSTRUCTIONS = 1,
    PERF_COUNT_HW_CACHE_REFERENCES = 2,
    PERF_COUNT_HW_CACHE_MISSES = 3,
    PERF_COUNT_HW_BRANCH_INSTRUCTIONS = 4,
    PERF_COUNT_HW_BRANCH_MISSES = 5,
    PERF_COUNT_HW_BUS_CYCLES = 6,
    PERF_COUNT_HW_STALLED_CYCLES_FRONTEND = 7,
    PERF_COUNT_HW_STALLED_CYCLES_BACKEND = 8,
    PERF_COUNT_HW_REF_CPU_CYCLES = 9,

    /// non-ABI
    PERF_COUNT_HW_MAX = 10,
}

/// Generalized hardware cache events:
///
/// { L1-D, L1-I, LLC, ITLB, DTLB, BPU, NODE } x
/// { read, write, prefetch } x
/// { accesses, misses }
#[repr(u32)]
pub enum perf_hw_cache_id_t {
    PERF_COUNT_HW_CACHE_L1D = 0,
    PERF_COUNT_HW_CACHE_L1I = 1,
    PERF_COUNT_HW_CACHE_LL = 2,
    PERF_COUNT_HW_CACHE_DTLB = 3,
    PERF_COUNT_HW_CACHE_ITLB = 4,
    PERF_COUNT_HW_CACHE_BPU = 5,
    PERF_COUNT_HW_CACHE_NODE = 6,

    /// non-ABI
    PERF_COUNT_HW_CACHE_MAX = 7,
}

#[repr(u32)]
pub enum perf_hw_cache_op_id_t {
    PERF_COUNT_HW_CACHE_OP_READ = 0,
    PERF_COUNT_HW_CACHE_OP_WRITE = 1,
    PERF_COUNT_HW_CACHE_OP_PREFETCH = 2,

    /// non-ABI
    PERF_COUNT_HW_CACHE_OP_MAX = 3,
}

#[repr(u32)]
pub enum perf_hw_cache_op_result_id_t {
    PERF_COUNT_HW_CACHE_RESULT_ACCESS = 0,
    PERF_COUNT_HW_CACHE_RESULT_MISS = 1,

    /// non-ABI
    PERF_COUNT_HW_CACHE_RESULT_MAX = 2,
}

/// Special "software" events provided by the kernel, even if the hardware
/// does not support performance events.
///
/// These events measure various physical and sw events of the kernel
/// (and allow the profiling of them as well):
#[repr(u32)]
pub enum perf_sw_ids_t {
    PERF_COUNT_SW_CPU_CLOCK = 0,
    PERF_COUNT_SW_TASK_CLOCK = 1,
    PERF_COUNT_SW_PAGE_FAULTS = 2,
    PERF_COUNT_SW_CONTEXT_SWITCHES = 3,
    PERF_COUNT_SW_CPU_MIGRATIONS = 4,
    PERF_COUNT_SW_PAGE_FAULTS_MIN = 5,
    PERF_COUNT_SW_PAGE_FAULTS_MAJ = 6,
    PERF_COUNT_SW_ALIGNMENT_FAULTS = 7,
    PERF_COUNT_SW_EMULATION_FAULTS = 8,
    PERF_COUNT_SW_DUMMY = 9,
    PERF_COUNT_SW_BPF_OUTPUT = 10,

    /// non-ABI
    PERF_COUNT_SW_MAX = 11,
}

/// Bits that can be set in `attr.sample_type` to request information
/// in the overflow packets.
#[repr(u64)]
#[derive(Debug, PartialOrd, PartialEq, Eq)]
pub enum perf_event_sample_format_t {
    PERF_SAMPLE_IP = 1,
    PERF_SAMPLE_TID = 1 << 1,
    PERF_SAMPLE_TIME = 1 << 2,
    PERF_SAMPLE_ADDR = 1 << 3,
    PERF_SAMPLE_READ = 1 << 4,
    PERF_SAMPLE_CALLCHAIN = 1 << 5,
    PERF_SAMPLE_ID = 1 << 6,
    PERF_SAMPLE_CPU = 1 << 7,
    PERF_SAMPLE_PERIOD = 1 << 8,
    PERF_SAMPLE_STREAM_ID = 1 << 9,
    PERF_SAMPLE_RAW = 1 << 10,
    PERF_SAMPLE_BRANCH_STACK = 1 << 11,
    PERF_SAMPLE_REGS_USER = 1 << 12,
    PERF_SAMPLE_STACK_USER = 1 << 13,
    PERF_SAMPLE_WEIGHT = 1 << 14,
    PERF_SAMPLE_DATA_SRC = 1 << 15,
    PERF_SAMPLE_IDENTIFIER = 1 << 16,
    PERF_SAMPLE_TRANSACTION = 1 << 17,
    PERF_SAMPLE_REGS_INTR = 1 << 18,
    PERF_SAMPLE_PHYS_ADDR = 1 << 19,

    PERF_SAMPLE_AUX = 1 << 20,
    PERF_SAMPLE_CGROUP = 1 << 21,
    PERF_SAMPLE_DATA_PAGE_SIZE = 1 << 22,
    PERF_SAMPLE_CODE_PAGE_SIZE = 1 << 23,
    PERF_SAMPLE_WEIGHT_STRUCT = 1 << 24,

    /// non-ABI
    PERF_SAMPLE_MAX = 1 << 25,

    // non-ABI; internal use
    __PERF_SAMPLE_CALLCHAIN_EARLY = 1 << 63,
}

pub const PERF_SAMPLE_WEIGHT_TYPE: u64 = perf_event_sample_format_t::PERF_SAMPLE_WEIGHT as u64
    | perf_event_sample_format_t::PERF_SAMPLE_WEIGHT_STRUCT as u64;

/// values to program into `branch_sample_type` when `PERF_SAMPLE_BRANCH` is set
///
/// If the user does not pass priv level information via `branch_sample_type`
/// the kernel uses the event's priv level. Branch and event priv levels do
/// not have to match. Branch priv level is checked for permissions.
///
/// The branch types can be combined, however `BRANCH_ANY` covers all types
/// of branches and therefore it supersedes all the other types.
/// user branches
#[repr(u32)]
pub enum perf_branch_sample_type_shift_t {
    PERF_SAMPLE_BRANCH_USER_SHIFT = 0,
    /// kernel branches
    PERF_SAMPLE_BRANCH_KERNEL_SHIFT = 1,
    /// hypervisor branches
    PERF_SAMPLE_BRANCH_HV_SHIFT = 2,

    /// any branch types
    PERF_SAMPLE_BRANCH_ANY_SHIFT = 3,
    /// any call branch
    PERF_SAMPLE_BRANCH_ANY_CALL_SHIFT = 4,
    /// any return branch
    PERF_SAMPLE_BRANCH_ANY_RETURN_SHIFT = 5,
    /// indirect calls
    PERF_SAMPLE_BRANCH_IND_CALL_SHIFT = 6,
    /// transaction aborts
    PERF_SAMPLE_BRANCH_ABORT_TX_SHIFT = 7,
    /// in transaction
    PERF_SAMPLE_BRANCH_IN_TX_SHIFT = 8,
    /// not in transaction
    PERF_SAMPLE_BRANCH_NO_TX_SHIFT = 9,
    /// conditional branches
    PERF_SAMPLE_BRANCH_COND_SHIFT = 10,

    /// call/ret stack
    PERF_SAMPLE_BRANCH_CALL_STACK_SHIFT = 11,
    /// indirect jumps
    PERF_SAMPLE_BRANCH_IND_JUMP_SHIFT = 12,
    /// direct call
    PERF_SAMPLE_BRANCH_CALL_SHIFT = 13,

    /// no flags
    PERF_SAMPLE_BRANCH_NO_FLAGS_SHIFT = 14,
    /// no cycles
    PERF_SAMPLE_BRANCH_NO_CYCLES_SHIFT = 15,

    /// save branch type
    PERF_SAMPLE_BRANCH_TYPE_SAVE_SHIFT = 16,

    // save low level index of raw branch records
    PERF_SAMPLE_BRANCH_HW_INDEX_SHIFT = 17,

    /// non-ABI
    PERF_SAMPLE_BRANCH_MAX_SHIFT = 18,
}

#[allow(clippy::enum_glob_use)]
use perf_branch_sample_type_shift_t::*;

#[repr(u32)]
pub enum perf_branch_sample_type_t {
    PERF_SAMPLE_BRANCH_USER = 1 << PERF_SAMPLE_BRANCH_USER_SHIFT as u32,
    PERF_SAMPLE_BRANCH_KERNEL = 1 << PERF_SAMPLE_BRANCH_KERNEL_SHIFT as u32,
    PERF_SAMPLE_BRANCH_HV = 1 << PERF_SAMPLE_BRANCH_HV_SHIFT as u32,

    PERF_SAMPLE_BRANCH_ANY = 1 << PERF_SAMPLE_BRANCH_ANY_SHIFT as u32,
    PERF_SAMPLE_BRANCH_ANY_CALL = 1 << PERF_SAMPLE_BRANCH_ANY_CALL_SHIFT as u32,
    PERF_SAMPLE_BRANCH_ANY_RETURN = 1 << PERF_SAMPLE_BRANCH_ANY_RETURN_SHIFT as u32,
    PERF_SAMPLE_BRANCH_IND_CALL = 1 << PERF_SAMPLE_BRANCH_IND_CALL_SHIFT as u32,
    PERF_SAMPLE_BRANCH_ABORT_TX = 1 << PERF_SAMPLE_BRANCH_ABORT_TX_SHIFT as u32,
    PERF_SAMPLE_BRANCH_IN_TX = 1 << PERF_SAMPLE_BRANCH_IN_TX_SHIFT as u32,
    PERF_SAMPLE_BRANCH_NO_TX = 1 << PERF_SAMPLE_BRANCH_NO_TX_SHIFT as u32,
    PERF_SAMPLE_BRANCH_COND = 1 << PERF_SAMPLE_BRANCH_COND_SHIFT as u32,

    PERF_SAMPLE_BRANCH_CALL_STACK = 1 << PERF_SAMPLE_BRANCH_CALL_STACK_SHIFT as u32,
    PERF_SAMPLE_BRANCH_IND_JUMP = 1 << PERF_SAMPLE_BRANCH_IND_JUMP_SHIFT as u32,
    PERF_SAMPLE_BRANCH_CALL = 1 << PERF_SAMPLE_BRANCH_CALL_SHIFT as u32,

    PERF_SAMPLE_BRANCH_NO_FLAGS = 1 << PERF_SAMPLE_BRANCH_NO_FLAGS_SHIFT as u32,
    PERF_SAMPLE_BRANCH_NO_CYCLES = 1 << PERF_SAMPLE_BRANCH_NO_CYCLES_SHIFT as u32,

    PERF_SAMPLE_BRANCH_TYPE_SAVE = 1 << PERF_SAMPLE_BRANCH_TYPE_SAVE_SHIFT as u32,

    PERF_SAMPLE_BRANCH_HW_INDEX = 1 << PERF_SAMPLE_BRANCH_HW_INDEX_SHIFT as u32,
    PERF_SAMPLE_BRANCH_MAX = 1 << PERF_SAMPLE_BRANCH_MAX_SHIFT as u32,
}

/// Common flow change classification
/// unknown
pub const PERF_BR_UNKNOWN: i32 = 0;
/// conditional
pub const PERF_BR_COND: i32 = 1;
/// unconditional
pub const PERF_BR_UNCOND: i32 = 2;
/// indirect
pub const PERF_BR_IND: i32 = 3;
/// function call
pub const PERF_BR_CALL: i32 = 4;
/// indirect function call
pub const PERF_BR_IND_CALL: i32 = 5;
/// function return
pub const PERF_BR_RET: i32 = 6;
/// syscall
pub const PERF_BR_SYSCALL: i32 = 7;
/// syscall return
pub const PERF_BR_SYSRET: i32 = 8;
/// conditional function call
pub const PERF_BR_COND_CALL: i32 = 9;
/// conditional function return
pub const PERF_BR_COND_RET: i32 = 10;
pub const PERF_BR_MAX: i32 = 11;

pub const PERF_SAMPLE_BRANCH_PLM_ALL: u32 = perf_branch_sample_type_t::PERF_SAMPLE_BRANCH_USER
    as u32
    | perf_branch_sample_type_t::PERF_SAMPLE_BRANCH_KERNEL as u32
    | perf_branch_sample_type_t::PERF_SAMPLE_BRANCH_HV as u32;

/// Values to determine ABI of the registers dump.
#[repr(u32)]
pub enum perf_sample_regs_abi_t {
    PERF_SAMPLE_REGS_ABI_NONE = 0,
    PERF_SAMPLE_REGS_ABI_32 = 1,
    PERF_SAMPLE_REGS_ABI_64 = 2,
}

/// Values for the memory transaction event qualifier, mostly for
/// abort events. Multiple bits can be set.
/// From elision
pub const PERF_TXN_ELISION: i32 = 1;
/// From transaction
pub const PERF_TXN_TRANSACTION: i32 = 1 << 1;
/// Instruction is related
pub const PERF_TXN_SYNC: i32 = 1 << 2;
/// Instruction not related
pub const PERF_TXN_ASYNC: i32 = 1 << 3;
/// Retry possible
pub const PERF_TXN_RETRY: i32 = 1 << 4;
/// Conflict abort
pub const PERF_TXN_CONFLICT: i32 = 1 << 5;
/// Capacity write abort
pub const PERF_TXN_CAPACITY_WRITE: i32 = 1 << 6;
/// Capacity read abort
pub const PERF_TXN_CAPACITY_READ: i32 = 1 << 7;

/// non-ABI
pub const PERF_TXN_MAX: i32 = 1 << 8;

/// bits 32..63 are reserved for the abort code
pub const PERF_TXN_ABORT_MASK: u64 = 0xffff_ffff << 32;

pub const PERF_TXN_ABORT_SHIFT: i32 = 32;

/// The format of the data returned by `read()` on a perf event fd
/// as specified by `attr.read_format`:
///
/// ```c
/// struct read_format {
///   { u64 value;
///     { u64 time_enabled; } && PERF_FORMAT_TOTAL_TIME_ENABLED
///     { u64 time_running; } && PERF_FORMAT_TOTAL_TIME_RUNNING
///     { u64 id; } && PERF_FORMAT_ID
///   } && !PERF_FORMAT_GROUP
///
///   { u64 nr;
///     { u64 time_enabled; } && PERF_FORMAT_TOTAL_TIME_ENABLED
///     { u64 time_running; } && PERF_FORMAT_TOTAL_TIME_RUNNING
///     { u64 value;
///       { u64 id; } && PERF_FORMAT_ID
///     } cntr[nr];
///   } && PERF_FORMAT_GROUP
/// };
/// ```
#[repr(u32)]
pub enum perf_event_read_format_t {
    PERF_FORMAT_TOTAL_TIME_ENABLED = 1,
    PERF_FORMAT_TOTAL_TIME_RUNNING = 1 << 1,
    PERF_FORMAT_ID = 1 << 2,
    PERF_FORMAT_GROUP = 1 << 3,

    /// non-ABI
    PERF_FORMAT_MAX = 1 << 4,
}

/// sizeof first published struct
pub const PERF_ATTR_SIZE_VER0: i32 = 64;
/// add: config2
pub const PERF_ATTR_SIZE_VER1: i32 = 72;
/// add: `branch_sample_type`
pub const PERF_ATTR_SIZE_VER2: i32 = 80;
/// add: `sample_regs_user`
/// add: `sample_stack_user`
pub const PERF_ATTR_SIZE_VER3: i32 = 96;
/// add: `sample_regs_intr`
pub const PERF_ATTR_SIZE_VER4: i32 = 104;
/// add: `aux_watermark`
pub const PERF_ATTR_SIZE_VER5: i32 = 112;

#[repr(C)]
pub union perf_event_attr_sample_t {
    pub sample_period: u64,
    pub sample_freq: u64,
}

#[repr(C)]
pub union perf_event_attr_wakeup_t {
    /// wakeup every n events
    pub wakeup_events: u32,

    /// bytes before wakeup
    pub wakeup_watermark: u32,
}

#[repr(C)]
pub union perf_event_attr_config1_t {
    pub bp_addr: u64,

    /// for `perf_kprobe`
    pub kprobe_func: u64,

    /// for `perf_uprobe`
    pub uprobe_path: u64,

    /// extension of config
    pub config1: u64,
}

#[repr(C)]
pub union perf_event_attr_config2_t {
    pub bp_len: u64,

    /// when `kprobe_func` == NULL
    pub kprobe_addr: u64,

    /// for `perf_k/u` probe
    pub probe_offset: u64,

    /// extension of config1
    pub config2: u64,
}

/// Hardware `event_id` to monitor via a performance monitoring event:
///
/// `@sample_max_stack`: Max number of frame pointers in a callchain
/// should be < `/proc/sys/kernel/perf_event_max_stack`
#[repr(C)]
pub struct perf_event_attr_t {
    /// Major type: `hardware/software/tracepoint/etc`.
    pub type_: u32,

    /// Size of the attr structure, for fwd/bwd compat.
    pub size: u32,

    /// Type specific configuration information.
    pub config: u64,

    pub sample: perf_event_attr_sample_t,

    pub sample_type: u64,
    pub read_format: u64,

    /// off by default
    //pub disabled       :  1,
    pub disabled: u8,

    /// children inherit it
    //pub inherit	       :  1,
    pub inherit: u8,
    /// must always be on PMU
    //pub pinned	       :  1,
    pub pinned: u8,
    /// only group on PMU
    //pub exclusive      :  1,
    pub exclusive: u8,
    /// don't count user
    //pub exclude_user   :  1,
    pub exclude_user: u8,
    /// ditto kernel
    //pub exclude_kernel :  1,
    pub exclude_kernel: u8,
    /// ditto hypervisor
    //pub exclude_hv     :  1,
    pub exclude_hv: u8,
    /// don't count when idle
    //pub exclude_idle   :  1,
    pub exclude_idle: u8,
    /// include mmap data
    //pub mmap           :  1,
    pub mmap: u8,
    /// include comm data
    //pub comm	       :  1,
    pub comm: u8,
    /// use freq, not period
    //pub freq           :  1,
    pub freq: u8,
    /// per task counts
    //pub inherit_stat   :  1,
    pub inherit_stat: u8,
    /// next exec enables
    //pub enable_on_exec :  1,
    pub enable_on_exec: u8,
    /// trace fork/exit
    //pub task           :  1,
    pub task: u8,
    /// `wakeup_watermark`
    //pub watermark      :  1,
    pub watermark: u8,

    /// `precise_ip`:
    /// 0 - `SAMPLE_IP` can have arbitrary skid
    /// 1 - `SAMPLE_IP` must have constant skid
    /// 2 - `SAMPLE_IP` requested to have 0 skid
    /// 3 - `SAMPLE_IP` must have 0 skid
    ///
    /// See also `PERF_RECORD_MISC_EXACT_IP`
    /// skid constraint
    //pub precise_ip     :  2,
    pub precise_ip: u8,
    /// non-exec mmap data
    //pub mmap_data      :  1,
    pub mmap_data: u8,
    /// `sample_type` all events
    //pub sample_id_all  :  1,
    pub sample_id_all: u8,

    /// don't count in host
    //pub exclude_host   :  1,
    pub exclude_host: u8,
    /// don't count in guest
    //pub exclude_guest  :  1,
    pub exclude_guest: u8,

    /// exclude kernel callchains
    //pub exclude_callchain_kernel : 1,
    pub exclude_callchain_kernel: u8,
    /// exclude user callchains
    //pub exclude_callchain_user   : 1,
    pub exclude_callchain_user: u8,
    /// include mmap with inode data
    //pub mmap2          :  1,
    pub mmap2: u8,
    /// flag comm events that are due to an exec
    //pub comm_exec      :  1,
    pub comm_exec: u8,
    /// use @clockid for time fields
    //pub use_clockid    :  1,
    pub use_clockid: u8,
    /// context switch data
    //pub context_switch :  1,
    pub context_switch: u8,
    /// Write ring buffer from end to beginning
    //pub write_backward :  1,
    pub write_backward: u8,
    /// include namespaces data
    //pub namespaces     :  1,
    pub namespaces: u8,
    //reserved_1   : 35,
    reserved_1: u8,

    pub wakeup: perf_event_attr_wakeup_t,

    pub bp_type: u32,

    pub config1: perf_event_attr_config1_t,
    pub config2: perf_event_attr_config2_t,

    /// enum `perf_branch_sample_type`
    pub branch_sample_type: u64,

    /// Defines set of user regs to dump on samples.
    /// See `asm/perf_regs.h` for details.
    pub sample_regs_user: u64,

    /// Defines size of the user stack to dump on samples.
    pub sample_stack_user: u32,

    pub clockid: i32,

    /// Defines set of regs to dump for each sample
    /// state captured on:
    /// - precise = 0: PMU interrupt
    /// - precise > 0: sampled instruction
    ///
    /// See `asm/perf_regs.h` for details.
    pub sample_regs_intr: u64,

    /// Wakeup watermark for AUX area
    pub aux_watermark: u32,
    pub sample_max_stack: u16,
    /// align to __u64
    reserved_2: u16,
}

/// Structure used by below `PERF_EVENT_IOC_QUERY_BPF` command
/// to query bpf programs attached to the same perf tracepoint
/// as the given perf event.
#[repr(C)]
pub struct perf_event_query_bpf_t {
    /// The below ids array length
    pub ids_len: u32,

    /// Set by the kernel to indicate the number of available programs
    pub prog_cnt: u32,

    /// User provided buffer to store program ids
    pub ids: *mut u32,
}

#[inline]
#[must_use]
pub const fn perf_flags(attr: &perf_event_attr_t) -> u64 {
    attr.read_format + 1
}

// Ioctls that can be done on a perf event fd:
//pub const PERF_EVENT_IOC_ENABLE			_IO ('$', 0)
//#define PERF_EVENT_IOC_DISABLE			_IO ('$', 1)
//#define PERF_EVENT_IOC_REFRESH			_IO ('$', 2)
//#define PERF_EVENT_IOC_RESET			_IO ('$', 3)
//#define PERF_EVENT_IOC_PERIOD			_IOW('$', 4, __u64)
//#define PERF_EVENT_IOC_SET_OUTPUT		_IO ('$', 5)
//#define PERF_EVENT_IOC_SET_FILTER		_IOW('$', 6, char *)
//#define PERF_EVENT_IOC_ID			_IOR('$', 7, __u64 *)
//#define PERF_EVENT_IOC_SET_BPF			_IOW('$', 8, __u32)
//#define PERF_EVENT_IOC_PAUSE_OUTPUT		_IOW('$', 9, __u32)
//#define PERF_EVENT_IOC_QUERY_BPF		_IOWR('$', 10, struct perf_event_query_bpf *)
//#define PERF_EVENT_IOC_MODIFY_ATTRIBUTES	_IOW('$', 11, struct perf_event_attr *)

pub const PERF_IOC_FLAG_GROUP: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct perf_event_mmap_page_cap_detail_t {
    /// Always 0, deprecated, see commit 860f085b74e9
    //pub cap_bit0		: 1,
    pub cap_bit0: u8,

    /// Always 1, signals that bit 0 is zero
    //pub cap_bit0_is_deprecated	: 1,
    pub cap_bit0_is_deprecated: u8,

    /// The RDPMC instruction can be used to read counts
    //pub cap_user_rdpmc		: 1,
    pub cap_user_rdpmc: u8,

    /// The time_* fields are used
    //pub cap_user_time		: 1,
    pub cap_user_time: u8,

    /// The `time_zero` field is used
    //pub cap_user_time_zero	: 1,
    pub cap_user_time_zero: u8,

    //cap_____res		: 59,
    cap_____res: u8,
}

#[repr(C)]
pub union perf_event_mmap_page_cap_t {
    pub capabilities: u64,
    pub detail: perf_event_mmap_page_cap_detail_t,
}

/// Structure of the page that can be mapped via mmap
#[repr(C)]
pub struct perf_event_mmap_page_t {
    /// version number of this structure
    pub version: u32,

    /// lowest version this is compat with
    pub compat_version: u32,

    /// Bits needed to read the hw events in user-space.
    ///
    /// ```c
    ///   u32 seq, time_mult, time_shift, index, width;
    ///   u64 count, enabled, running;
    ///   u64 cyc, time_offset;
    ///   s64 pmc = 0;
    ///
    ///   do {
    ///     seq = pc->lock;
    ///     barrier()
    ///
    ///     enabled = pc->time_enabled;
    ///     running = pc->time_running;
    ///
    ///     if (pc->cap_usr_time && enabled != running) {
    ///       cyc = rdtsc();
    ///       time_offset = pc->time_offset;
    ///       time_mult   = pc->time_mult;
    ///       time_shift  = pc->time_shift;
    ///     }
    ///
    ///     index = pc->index;
    ///     count = pc->offset;
    ///     if (pc->cap_user_rdpmc && index) {
    ///       width = pc->pmc_width;
    ///       pmc = rdpmc(index - 1);
    ///     }
    ///
    ///     barrier();
    ///   } while (pc->lock != seq);
    ///   ```
    ///
    /// NOTE: for obvious reason this only works on self-monitoring
    ///       processes.
    /// seqlock for synchronization
    pub lock: u32,
    /// hardware event identifier
    pub index: u32,
    /// add to hardware event value
    pub offset: i64,
    /// time event active
    pub time_enabled: u64,
    /// time event on cpu
    pub time_running: u64,

    pub cap: perf_event_mmap_page_cap_t,

    /// If `cap_user_rdpmc` this field provides the bit-width of the value
    /// read using the `rdpmc()` or equivalent instruction. This can be used
    /// to sign extend the result like:
    ///
    ///   pmc <<= 64 - width;
    ///   pmc >>= 64 - width; // signed shift right
    ///   count += pmc;
    pub pmc_width: u16,

    /// If `cap_usr_time` the below fields can be used to compute the time
    /// delta since `time_enabled` (in ns) using rdtsc or similar.
    ///
    ///   u64 quot, rem;
    ///   u64 delta;
    ///
    ///   quot = (cyc >> `time_shift`);
    ///   rem = cyc & (((u64)1 << `time_shift`) - 1);
    ///   delta = `time_offset` + quot * `time_mult` +
    ///              ((rem * `time_mult`) >> `time_shift`);
    ///
    /// Where `time_offset,time_mult,time_shift` and cyc are read in the
    /// seqcount loop described above. This delta can then be added to
    /// enabled and possible running (if index), improving the scaling:
    ///
    ///   enabled += delta;
    ///   if (index)
    ///     running += delta;
    ///
    ///   quot = count / running;
    ///   rem  = count % running;
    ///   count = quot * enabled + (rem * enabled) / running;
    pub time_shift: u16,
    pub time_mult: u32,
    pub time_offset: u64,

    /// If `cap_usr_time_zero`, the hardware clock (e.g. TSC) can be calculated
    /// from sample timestamps.
    ///
    ///   time = timestamp - `time_zero`;
    ///   quot = time / `time_mult`;
    ///   rem  = time % `time_mult`;
    ///   cyc = (quot << `time_shift`) + (rem << `time_shift`) / `time_mult`;
    ///
    /// And vice versa:
    ///
    ///   quot = cyc >> `time_shift`;
    ///   rem  = cyc & (((u64)1 << `time_shift`) - 1);
    ///   timestamp = `time_zero` + quot * `time_mult` +
    ///               ((rem * `time_mult`) >> `time_shift`);
    pub time_zero: u64,
    /// Header size up to __reserved[] fields.
    pub size: u32,

    /// Hole for extension of the self monitor capabilities
    /// align to 1k.
    reserved: [u8; 118 * 8 + 4],

    /// Control data for the `mmap()` data buffer.
    ///
    /// User-space reading the @`data_head` value should issue an `smp_rmb()`
    /// after reading this value.
    ///
    /// When the mapping is `PROT_WRITE` the @`data_tail` value should be
    /// written by userspace to reflect the last read data, after issueing
    /// an `smp_mb()` to separate the data read from the ->`data_tail` store.
    /// In this case the kernel will not over-write unread data.
    ///
    /// See `perf_output_put_handle()` for the data ordering.
    ///
    /// data_{offset,size} indicate the location and size of the perf record
    /// buffer within the mmapped area.
    /// head in the data section
    pub data_head: u64,
    /// user-space written tail
    pub data_tail: u64,
    /// where the buffer starts
    pub data_offset: u64,
    /// data buffer size
    pub data_size: u64,

    /// AUX area is defined by aux_{offset,size} fields that should be set
    /// by the userspace, so that
    ///
    /// `aux_offset` >= `data_offset` + `data_size`
    ///
    /// prior to mmap()ing it. Size of the mmap()ed area should be `aux_size`.
    ///
    /// Ring buffer pointers aux_{head,tail} have the same semantics as
    /// data_{head,tail} and same ordering rules apply.
    pub aux_head: u64,
    pub aux_tail: u64,
    pub aux_offset: u64,
    pub aux_size: u64,
}

pub const PERF_RECORD_MISC_CPUMODE_MASK: i32 = 7;
pub const PERF_RECORD_MISC_CPUMODE_UNKNOWN: i32 = 0;
pub const PERF_RECORD_MISC_KERNEL: i32 = 1;
pub const PERF_RECORD_MISC_USER: i32 = 2;
pub const PERF_RECORD_MISC_HYPERVISOR: i32 = 3;
pub const PERF_RECORD_MISC_GUEST_KERNEL: i32 = 4;
pub const PERF_RECORD_MISC_GUEST_USER: i32 = 5;

/// Indicates that `/proc/PID/maps` parsing are truncated by time out.
pub const PERF_RECORD_MISC_PROC_MAP_PARSE_TIMEOUT: i32 = 1 << 12;

/// Following `PERF_RECORD_MISC_*` are used on different
/// events, so can reuse the same bit position:
///
/// - `PERF_RECORD_MISC_MMAP_DATA`, `PERF_RECORD_MMAP*` events
/// - `PERF_RECORD_MISC_COMM_EXEC`, `PERF_RECORD_COMM` event
/// - `PERF_RECORD_MISC_SWITCH_OUT`, `PERF_RECORD_SWITCH*` events
pub const PERF_RECORD_MISC_MMAP_DATA: i32 = 1 << 13;
pub const PERF_RECORD_MISC_COMM_EXEC: i32 = 1 << 13;
pub const PERF_RECORD_MISC_SWITCH_OUT: i32 = 1 << 13;

/// These `PERF_RECORD_MISC_*` flags below are safely reused
/// for the following events:
///
/// - `PERF_RECORD_MISC_EXACT_IP`: `PERF_RECORD_SAMPLE` of precise events
///
/// - `PERF_RECORD_MISC_SWITCH_OUT_PREEMPT`: `PERF_RECORD_SWITCH*` events
///
/// - `PERF_RECORD_MISC_EXACT_IP`:
///   Indicates that the content of `PERF_SAMPLE_IP` points to
///   the actual instruction that triggered the event. See also
///   `perf_event_attr::precise_ip`.
///
/// - `PERF_RECORD_MISC_SWITCH_OUT_PREEMPT`:
///   Indicates that thread was preempted in `TASK_RUNNING` state.
pub const PERF_RECORD_MISC_EXACT_IP: i32 = 1 << 14;
pub const PERF_RECORD_MISC_SWITCH_OUT_PREEMPT: i32 = 1 << 14;

/// Reserve the last bit to indicate some extended misc field
pub const PERF_RECORD_MISC_EXT_RESERVED: i32 = 1 << 15;

#[repr(C)]
pub struct perf_event_header_t {
    pub type_: u32,
    pub misc: u16,
    pub size: u16,
}

#[repr(C)]
pub struct perf_ns_link_info_t {
    pub dev: u64,
    pub ino: u64,
}

pub const NET_NS_INDEX: i32 = 0;
pub const UTS_NS_INDEX: i32 = 1;
pub const IPC_NS_INDEX: i32 = 2;
pub const PID_NS_INDEX: i32 = 3;
pub const USER_NS_INDEX: i32 = 4;
pub const MNT_NS_INDEX: i32 = 5;
pub const CGROUP_NS_INDEX: i32 = 6;

/// number of available namespaces
pub const NR_NAMESPACES: i32 = 7;

/// `perf_event_type`
pub enum perf_event_type_t {
    /// If `perf_event_attr.sample_id_all` is set then all event types will
    /// have the `sample_type` selected fields related to where/when
    /// (identity) an event took place (TID, TIME, ID, `STREAM_ID`, CPU
    /// IDENTIFIER) described in `PERF_RECORD_SAMPLE` below, it will be stashed
    /// just after the `perf_event_header` and the fields already present for
    /// the existing fields, i.e. at the end of the payload. That way a newer
    /// perf.data file will be supported by older perf tools, with these new
    /// optional fields being ignored.
    ///
    /// struct `sample_id` {
    ///   { u32 pid, tid; } && `PERF_SAMPLE_TID`
    ///   { u64 time; } && `PERF_SAMPLE_TIME`
    ///   { u64 id; } && `PERF_SAMPLE_ID`
    ///   { u64 `stream_id`;} && `PERF_SAMPLE_STREAM_ID`
    ///   { u32 cpu, res; } && `PERF_SAMPLE_CPU`
    ///   { u64 id; } && `PERF_SAMPLE_IDENTIFIER`
    /// } && `perf_event_attr::sample_id_all`
    ///
    /// Note that `PERF_SAMPLE_IDENTIFIER` duplicates `PERF_SAMPLE_ID`.  The
    /// advantage of `PERF_SAMPLE_IDENTIFIER` is that its position is fixed
    /// relative to header.size.
    ///
    ///
    /// The MMAP events record the `PROT_EXEC` mappings so that we can
    /// correlate userspace IPs to code. They have the following structure:
    ///
    /// struct {
    ///   struct `perf_event_header` header;
    ///
    ///   u32 pid, tid;
    ///   u64 addr;
    ///   u64 len;
    ///   u64 pgoff;
    ///   char filename[];
    ///   struct `sample_id` `sample_id`;
    /// };
    PERF_RECORD_MMAP = 1,

    /// struct {
    ///   struct `perf_event_header` header;
    ///   u64 id;
    ///   u64 lost;
    ///   struct `sample_id` `sample_id`;
    /// };
    PERF_RECORD_LOST = 2,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///
    ///   u32 pid, tid;
    ///   char comm[];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_COMM = 3,

    /// struct {
    ///   struct `perf_event_header` header;
    ///   u32 pid, ppid;
    ///   u32 tid, ptid;
    ///   u64 time;
    ///   struct `sample_id` `sample_id`;
    /// };
    PERF_RECORD_EXIT = 4,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u64 time;
    ///   u64 id;
    ///   u64 stream_id;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_THROTTLE = 5,

    PERF_RECORD_UNTHROTTLE = 6,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u32 pid, ppid;
    ///   u32 tid, ptid;
    ///   u64 time;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_FORK = 7,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u32 pid, tid;
    ///
    ///   struct read_format values;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_READ = 8,

    ///  ```c
    /// struct {
    ///   struct perf_event_header header;
    ///
    ///   #
    ///   # Note that PERF_SAMPLE_IDENTIFIER duplicates PERF_SAMPLE_ID.
    ///   # The advantage of PERF_SAMPLE_IDENTIFIER is that its position
    ///   # is fixed relative to header.
    ///   #
    ///   { u64 id; } && PERF_SAMPLE_IDENTIFIER
    ///   { u64 ip; } && PERF_SAMPLE_IP
    ///   { u32 pid, tid; } && PERF_SAMPLE_TID
    ///   { u64 time; } && PERF_SAMPLE_TIME
    ///   { u64 addr; } && PERF_SAMPLE_ADDR
    ///   { u64 id; } && PERF_SAMPLE_ID
    ///   { u64 stream_id; } && PERF_SAMPLE_STREAM_ID
    ///   { u32 cpu, res; } && PERF_SAMPLE_CPU
    ///   { u64 period; } && PERF_SAMPLE_PERIOD
    ///
    ///   { struct read_format values; } && PERF_SAMPLE_READ
    ///
    ///   { u64 nr;
    ///     u64 ips[nr]; } && PERF_SAMPLE_CALLCHAIN
    ///
    ///   #
    ///   # The RAW record below is opaque data wrt the ABI
    ///   #
    ///   # That is, the ABI doesn't make any promises wrt to
    ///   # the stability of its content, it may vary depending
    ///   # on event, hardware, kernel version and phase of
    ///   # the moon.
    ///   #
    ///   # In other words, PERF_SAMPLE_RAW contents are not an ABI.
    ///   #
    ///
    ///   { u32 size;
    ///     char data[size]; }&& PERF_SAMPLE_RAW
    ///
    ///   { u64 nr;
    ///     { u64 from, to, flags } lbr[nr];} && PERF_SAMPLE_BRANCH_STACK
    ///
    ///   { u64 abi; # enum perf_sample_regs_abi
    ///     u64 regs[weight(mask)]; } && PERF_SAMPLE_REGS_USER
    //
    ///   { u64 size;
    ///     char data[size];
    ///     u64 dyn_size; } && PERF_SAMPLE_STACK_USER
    ///
    ///   { u64 weight; } && PERF_SAMPLE_WEIGHT
    ///   { u64 data_src; } && PERF_SAMPLE_DATA_SRC
    ///   { u64 transaction; } && PERF_SAMPLE_TRANSACTION
    ///   { u64 abi; # enum perf_sample_regs_abi
    ///     u64 regs[weight(mask)]; } && PERF_SAMPLE_REGS_INTR
    ///   { u64 phys_addr;} && PERF_SAMPLE_PHYS_ADDR
    /// };
    /// ```
    PERF_RECORD_SAMPLE = 9,

    /// The MMAP2 records are an augmented version of MMAP, they add
    /// maj, min, ino numbers to be used to uniquely identify each mapping
    ///
    /// struct {
    ///   struct `perf_event_header` header;
    ///   u32 pid, tid;
    ///   u64 addr;
    ///   u64 len;
    ///   u64 pgoff;
    ///   u32 maj;
    ///   u32 min;
    ///   u64 ino;
    ///   u64 `ino_generation`;
    ///   u32 prot, flags;
    ///   char filename[];
    ///   struct `sample_id` `sample_id`;
    /// };
    PERF_RECORD_MMAP2 = 10,

    /// Records that new data landed in the AUX buffer part.
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///
    ///   u64 aux_offset;
    ///   u64 aux_size;
    ///   u64 flags;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_AUX = 11,

    /// Indicates that instruction trace has started
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u32 pid;
    ///   u32 tid;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_ITRACE_START = 12,

    /// Records the dropped/lost sample number.
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///
    ///   u64 lost;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_LOST_SAMPLES = 13,

    /// Records a context switch in or out (flagged by
    /// `PERF_RECORD_MISC_SWITCH_OUT`). See also
    /// `PERF_RECORD_SWITCH_CPU_WIDE`.
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_SWITCH = 14,

    /// CPU-wide version of `PERF_RECORD_SWITCH` with `next_prev_pid` and
    /// `next_prev_tid` that are the next (switching out) or previous
    /// (switching in) pid/tid.
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u32 next_prev_pid;
    ///   u32 next_prev_tid;
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_SWITCH_CPU_WIDE = 15,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u32 pid;
    ///   u32 tid;
    ///   u64 nr_namespaces;
    ///   { u64 dev, inode; } [nr_namespaces];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_NAMESPACES = 16,

    /// Record ksymbol register/unregister events:
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u64 addr;
    ///   u32 len;
    ///   u16 ksym_type;
    ///   u16 flags;
    ///   char name[];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_KSYMBOL = 17,

    /// Record bpf events:
    /// ```c
    /// enum perf_bpf_event_type {
    ///   PERF_BPF_EVENT_UNKNOWN     = 0,
    ///   PERF_BPF_EVENT_PROG_LOAD   = 1,
    ///   PERF_BPF_EVENT_PROG_UNLOAD = 2,
    /// };
    ///
    /// struct {
    ///   struct perf_event_header header;
    ///   u16 type;
    ///   u16 flags;
    ///   u32 id;
    ///   u8 tag[BPF_TAG_SIZE];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_BPF_EVENT = 18,

    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u64 id;
    ///   char path[];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_CGROUP = 19,

    /// Records changes to kernel text i.e. self-modified code. '`old_len`' is
    /// the number of old bytes, '`new_len`' is the number of new bytes. Either
    /// '`old_len`' or '`new_len`' may be zero to indicate, for example, the
    /// addition or removal of a trampoline. 'bytes' contains the old bytes
    /// followed immediately by the new bytes.
    ///
    /// ```c
    /// struct {
    ///   struct perf_event_header header;
    ///   u64 addr;
    ///   u16 old_len;
    ///   u16 new_len;
    ///   u8 bytes[];
    ///   struct sample_id sample_id;
    /// };
    /// ```
    PERF_RECORD_TEXT_POKE = 20,

    /// non-ABI
    PERF_RECORD_MAX,
}

#[repr(u32)]
pub enum perf_record_ksymbol_type_t {
    PERF_RECORD_KSYMBOL_TYPE_UNKNOWN = 0,
    PERF_RECORD_KSYMBOL_TYPE_BPF = 1,

    /// Out of line code such as kprobe-replaced instructions or optimized
    /// kprobes or ftrace trampolines.
    PERF_RECORD_KSYMBOL_TYPE_OOL = 2,

    /// non-ABI
    PERF_RECORD_KSYMBOL_TYPE_MAX,
}

pub const PERF_RECORD_KSYMBOL_FLAGS_UNREGISTER: u8 = 1 << 0;

#[repr(u32)]
pub enum perf_bpf_event_type_t {
    PERF_BPF_EVENT_UNKNOWN = 0,
    PERF_BPF_EVENT_PROG_LOAD = 1,
    PERF_BPF_EVENT_PROG_UNLOAD = 2,

    /// non-ABI
    PERF_BPF_EVENT_MAX,
}

pub const PERF_MAX_STACK_DEPTH: i32 = 127;
pub const PERF_MAX_CONTEXTS_PER_STACK: i32 = 8;

#[repr(i64)]
pub enum perf_callchain_context_t {
    PERF_CONTEXT_HV = -32,
    PERF_CONTEXT_KERNEL = -128,
    PERF_CONTEXT_USER = -512,

    PERF_CONTEXT_GUEST = -2048,
    PERF_CONTEXT_GUEST_KERNEL = -2176,
    PERF_CONTEXT_GUEST_USER = -2560,
    PERF_CONTEXT_MAX = -4095,
}

/// `PERF_RECORD_AUX::flags` bits
/// record was truncated to fit
pub const PERF_AUX_FLAG_TRUNCATED: i32 = 0x01;
/// snapshot from overwrite mode
pub const PERF_AUX_FLAG_OVERWRITE: i32 = 0x02;
/// record contains gaps
pub const PERF_AUX_FLAG_PARTIAL: i32 = 0x04;
/// sample collided with another
pub const PERF_AUX_FLAG_COLLISION: i32 = 0x08;

pub const PERF_FLAG_FD_NO_GROUP: usize = 1;
pub const PERF_FLAG_FD_OUTPUT: usize = 1 << 1;
/// pid=cgroup id, per-cpu mode only
pub const PERF_FLAG_PID_CGROUP: usize = 1 << 2;
/// `O_CLOEXEC`
pub const PERF_FLAG_FD_CLOEXEC: usize = 1 << 3;

#[cfg(target_endian = "little")]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct perf_mem_data_mem_t {
    /// type of opcode
    //pub mem_op:5,
    pub mem_op: u8,

    /// memory hierarchy level
    //pub mem_lvl:14,
    pub mem_lvl: u8,

    /// snoop mode
    //pub mem_snoop:5,
    pub mem_snoop: u8,

    /// lock instr
    //pub mem_lock:2,
    pub mem_lock: u8,

    /// tlb access
    //pub mem_dtlb:7,
    pub mem_dtlb: u8,

    /// memory hierarchy level number
    //pub mem_lvl_num:4,
    pub mem_lvl_num: u8,

    /// remote
    //pub mem_remote:1,
    pub mem_remote: u8,

    /// snoop mode, ext
    //pub mem_snoopx:2,
    pub mem_snoopx: u8,

    //pub mem_rsvd:24,
    pub mem_rsvd: u8,
}

#[cfg(target_endian = "big")]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct perf_mem_data_mem_t {
    //pub mem_rsvd:24,
    pub mem_rsvd: u8,

    /// snoop mode, ext
    //pub mem_snoopx:2,
    pub mem_snoopx: u8,

    /// remote
    //pub mem_remote:1,
    pub mem_remote: u8,

    /// memory hierarchy level number
    //pub mem_lvl_num:4,
    pub mem_lvl_num: u8,

    /// tlb access
    //pub mem_dtlb:7,
    pub mem_dtlb: u8,

    /// lock instr
    //pub mem_lock:2,
    pub mem_lock: u8,

    /// snoop mode
    //pub mem_snoop:5,
    pub mem_snoop: u8,

    /// memory hierarchy level
    //pub mem_lvl:14,
    pub mem_lvl: u8,

    /// type of opcode
    //pub mem_op:5,
    pub mem_op: u8,
}

#[repr(C)]
pub union perf_mem_data_src_t {
    pub val: u64,
    pub mem: perf_mem_data_mem_t,
}

/// type of opcode (load/store/prefetch,code)
/// not available
pub const PERF_MEM_OP_NA: i32 = 0x01;
/// load instruction
pub const PERF_MEM_OP_LOAD: i32 = 0x02;
/// store instruction
pub const PERF_MEM_OP_STORE: i32 = 0x04;
/// prefetch
pub const PERF_MEM_OP_PFETCH: i32 = 0x08;
/// code (execution)
pub const PERF_MEM_OP_EXEC: i32 = 0x10;
pub const PERF_MEM_OP_SHIFT: i32 = 0;

/// memory hierarchy (memory level, hit or miss)
/// not available
pub const PERF_MEM_LVL_NA: i32 = 0x01;
/// hit level
pub const PERF_MEM_LVL_HIT: i32 = 0x02;
/// miss level
pub const PERF_MEM_LVL_MISS: i32 = 0x04;
/// L1
pub const PERF_MEM_LVL_L1: i32 = 0x08;
/// Line Fill Buffer
pub const PERF_MEM_LVL_LFB: i32 = 0x10;
/// L2
pub const PERF_MEM_LVL_L2: i32 = 0x20;
/// L3
pub const PERF_MEM_LVL_L3: i32 = 0x40;
/// Local DRAM
pub const PERF_MEM_LVL_LOC_RAM: i32 = 0x80;
/// Remote DRAM (1 hop)
pub const PERF_MEM_LVL_REM_RAM1: i32 = 0x100;
/// Remote DRAM (2 hops)
pub const PERF_MEM_LVL_REM_RAM2: i32 = 0x200;
/// Remote Cache (1 hop)
pub const PERF_MEM_LVL_REM_CCE1: i32 = 0x400;
/// Remote Cache (2 hops)
pub const PERF_MEM_LVL_REM_CCE2: i32 = 0x800;
/// I/O memory
pub const PERF_MEM_LVL_IO: i32 = 0x1000;
/// Uncached memory
pub const PERF_MEM_LVL_UNC: i32 = 0x2000;
pub const PERF_MEM_LVL_SHIFT: i32 = 5;

/// Remote
pub const PERF_MEM_REMOTE_REMOTE: i32 = 0x01;
pub const PERF_MEM_REMOTE_SHIFT: i32 = 37;

/// L1
pub const PERF_MEM_LVLNUM_L1: i32 = 0x01;
/// L2
pub const PERF_MEM_LVLNUM_L2: i32 = 0x02;
/// L3
pub const PERF_MEM_LVLNUM_L3: i32 = 0x03;
/// L4
pub const PERF_MEM_LVLNUM_L4: i32 = 0x04;
/// 5-0xa available
/// Any cache
pub const PERF_MEM_LVLNUM_ANY_CACHE: i32 = 0x0b;
/// LFB
pub const PERF_MEM_LVLNUM_LFB: i32 = 0x0c;
/// RAM
pub const PERF_MEM_LVLNUM_RAM: i32 = 0x0d;
/// PMEM
pub const PERF_MEM_LVLNUM_PMEM: i32 = 0x0e;
/// N/A
pub const PERF_MEM_LVLNUM_NA: i32 = 0x0f;

pub const PERF_MEM_LVLNUM_SHIFT: i32 = 33;

/// snoop mode
/// not available
pub const PERF_MEM_SNOOP_NA: i32 = 0x01;
/// no snoop
pub const PERF_MEM_SNOOP_NONE: i32 = 0x02;
/// snoop hit
pub const PERF_MEM_SNOOP_HIT: i32 = 0x04;
/// snoop miss
pub const PERF_MEM_SNOOP_MISS: i32 = 0x08;
/// snoop hit modified
pub const PERF_MEM_SNOOP_HITM: i32 = 0x10;
pub const PERF_MEM_SNOOP_SHIFT: i32 = 19;

/// forward
pub const PERF_MEM_SNOOPX_FWD: i32 = 0x01;
/// 1 free
pub const PERF_MEM_SNOOPX_SHIFT: i32 = 37;

/// locked instruction
/// not available
pub const PERF_MEM_LOCK_NA: i32 = 0x01;
/// locked transaction
pub const PERF_MEM_LOCK_LOCKED: i32 = 0x02;
pub const PERF_MEM_LOCK_SHIFT: i32 = 24;

/// TLB access
/// not available
pub const PERF_MEM_TLB_NA: i32 = 0x01;
/// hit level
pub const PERF_MEM_TLB_HIT: i32 = 0x02;
/// miss level
pub const PERF_MEM_TLB_MISS: i32 = 0x04;
/// L1
pub const PERF_MEM_TLB_L1: i32 = 0x08;
/// L2
pub const PERF_MEM_TLB_L2: i32 = 0x10;
/// Hardware Walker
pub const PERF_MEM_TLB_WK: i32 = 0x20;
/// OS fault handler
pub const PERF_MEM_TLB_OS: i32 = 0x40;
pub const PERF_MEM_TLB_SHIFT: i32 = 26;

//#define PERF_MEM_S(a, s) (((__u64)PERF_MEM_##a##_##s) << PERF_MEM_##a##_SHIFT)

/// single taken branch record layout:
///
/// - from: source instruction (may not always be a branch insn)
/// - to: branch target
/// - mispred: branch target was mispredicted
/// - predicted: branch target was predicted
///
/// support for mispred, predicted is optional. In case it
/// is not supported mispred = predicted = 0.
///
/// - `in_tx`: running in a hardware transaction
/// - `abort`: aborting a hardware transaction
/// - `cycles`: cycles from last branch (or 0 if not supported)
/// - `type`: branch type
#[repr(C)]
pub struct perf_branch_entry_t {
    pub from: u64,
    pub to: u64,

    /// target mispredicted
    //pub mispred:1,
    pub mispred: u8,

    /// target predicted
    //pub predicted:1,
    pub predicted: u8,

    /// in transaction
    //pub in_tx:1,
    pub in_tx: u8,

    /// transaction abort
    //pub abort:1,
    pub abort: u8,

    /// cycle count to last branch
    //pub cycles:16,
    pub cycles: u8,

    /// branch type
    //pub type_:4,
    pub type_: u8,

    //reserved:40,
    reserved: u8,
}
