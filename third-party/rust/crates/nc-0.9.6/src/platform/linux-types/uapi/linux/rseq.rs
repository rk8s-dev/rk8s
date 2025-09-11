// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/rseq.h`
//!
//! Restartable sequences system call API

#![allow(clippy::module_name_repetitions)]

pub const RSEQ_CPU_ID_UNINITIALIZED: i32 = -1;
pub const RSEQ_CPU_ID_REGISTRATION_FAILED: i32 = -2;

pub const RSEQ_FLAG_UNREGISTER: i32 = 1;

pub const RSEQ_CS_FLAG_NO_RESTART_ON_PREEMPT_BIT: i32 = 0;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_SIGNAL_BIT: i32 = 1;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_MIGRATE_BIT: i32 = 2;

pub const RSEQ_CS_FLAG_NO_RESTART_ON_PREEMPT: u32 = 1 << RSEQ_CS_FLAG_NO_RESTART_ON_PREEMPT_BIT;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_SIGNAL: u32 = 1 << RSEQ_CS_FLAG_NO_RESTART_ON_SIGNAL_BIT;
pub const RSEQ_CS_FLAG_NO_RESTART_ON_MIGRATE: u32 = 1 << RSEQ_CS_FLAG_NO_RESTART_ON_MIGRATE_BIT;

/// struct `rseq_cs` is aligned on 4 * 8 bytes to ensure it is always
/// contained within a single cache-line. It is usually declared as
/// link-time constant data.
// TODO(Shaohua): alignment __attribute__((aligned(4 * sizeof(__u64))));
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct rseq_cs_t {
    /// Version of this structure.
    pub version: u32,

    /// enum `rseq_cs_flags`
    pub flags: u32,

    pub start_ip: u64,

    /// Offset from `start_ip`.
    pub post_commit_offset: u64,

    pub abort_ip: u64,
}

#[cfg(target_endian = "big")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct rseq_cs_ptr_t {
    /// Initialized to zero.
    pub padding: u32,

    pub ptr32: u32,
}

#[cfg(target_endian = "little")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct rseq_cs_ptr_t {
    pub ptr32: u32,

    /// Initialized to zero.
    pub padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union rseq_cs_union_t {
    pub ptr64: u64,
    pub ptr: rseq_cs_ptr_t,
    //#ifdef __LP64__
    //__u64 ptr;
    //#else
}

/// struct rseq is aligned on 4 * 8 bytes to ensure it is always
/// contained within a single cache-line.
///
/// A single struct rseq per thread is allowed.
// TODO(Shaohua): alignment __attribute__((aligned(4 * sizeof(__u64))));
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rseq_t {
    /// Restartable sequences `cpu_id_start` field. Updated by the
    /// kernel. Read by user-space with single-copy atomicity
    /// semantics. This field should only be read by the thread which
    /// registered this data structure. Aligned on 32-bit. Always
    /// contains a value in the range of possible CPUs, although the
    /// value may not be the actual current CPU (e.g. if rseq is not
    /// initialized). This CPU number value should always be compared
    /// against the value of the `cpu_id` field before performing a rseq
    /// commit or returning a value read from a data structure indexed
    /// using the `cpu_id_start` value.
    pub cpu_id_start: u32,

    /// Restartable sequences `cpu_id` field. Updated by the kernel.
    /// Read by user-space with single-copy atomicity semantics. This
    /// field should only be read by the thread which registered this
    /// data structure. Aligned on 32-bit. Values
    /// `RSEQ_CPU_ID_UNINITIALIZED` and `RSEQ_CPU_ID_REGISTRATION_FAILED`
    /// have a special semantic: the former means "rseq uninitialized",
    /// and latter means "rseq initialization failed". This value is
    /// meant to be read within rseq critical sections and compared
    /// with the `cpu_id_start` value previously read, before performing
    /// the commit instruction, or read and compared with the
    /// `cpu_id_start` value before returning a value loaded from a data
    /// structure indexed using the `cpu_id_start` value.
    pub cpu_id: u32,

    /// Restartable sequences `rseq_cs` field.
    ///
    /// Contains NULL when no critical section is active for the current
    /// thread, or holds a pointer to the currently active struct `rseq_cs`.
    ///
    /// Updated by user-space, which sets the address of the currently
    /// active `rseq_cs` at the beginning of assembly instruction sequence
    /// block, and set to NULL by the kernel when it restarts an assembly
    /// instruction sequence block, as well as when the kernel detects that
    /// it is preempting or delivering a signal outside of the range
    /// targeted by the `rseq_cs`. Also needs to be set to NULL by user-space
    /// before reclaiming memory that contains the targeted struct `rseq_cs`.
    ///
    /// Read and set by the kernel. Set by user-space with single-copy
    /// atomicity semantics. This field should only be updated by the
    /// thread which registered this data structure. Aligned on 64-bit.
    pub rseq_cs: rseq_cs_union_t,

    /// Restartable sequences flags field.
    ///
    /// This field should only be updated by the thread which
    /// registered this data structure. Read by the kernel.
    /// Mainly used for single-stepping through rseq critical sections
    /// with debuggers.
    ///
    /// - `RSEQ_CS_FLAG_NO_RESTART_ON_PREEMPT`
    ///     Inhibit instruction sequence block restart on preemption
    ///     for this thread.
    /// - `RSEQ_CS_FLAG_NO_RESTART_ON_SIGNAL`
    ///     Inhibit instruction sequence block restart on signal
    ///     delivery for this thread.
    /// - `RSEQ_CS_FLAG_NO_RESTART_ON_MIGRATE`
    ///     Inhibit instruction sequence block restart on migration for
    ///     this thread.
    pub flags: u32,
}
