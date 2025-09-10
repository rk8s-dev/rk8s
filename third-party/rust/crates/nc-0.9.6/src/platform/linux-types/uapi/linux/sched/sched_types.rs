// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/sched/types.h`

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sched_param_t {
    pub sched_priority: i32,
}

/// sizeof first published struct
pub const SCHED_ATTR_SIZE_VER0: i32 = 48;
/// add: `util_{min,max}`
pub const SCHED_ATTR_SIZE_VER1: i32 = 56;

/// Extended scheduling parameters data structure.
///
/// This is needed because the original struct `sched_param` can not be
/// altered without introducing ABI issues with legacy applications
/// (e.g., in `sched_getparam()`).  
///
/// However, the possibility of specifying more than just a priority for
/// the tasks may be useful for a wide variety of application fields, e.g.,
/// multimedia, streaming, automation and control, and many others.
///
/// This variant (`sched_attr`) allows to define additional attributes to
/// improve the scheduler knowledge about task requirements.
///
/// Scheduling Class Attributes
/// ===========================
///
/// A subset of `sched_attr` attributes specifies the
/// scheduling policy and relative POSIX attributes:
///
/// @size size of the structure, for fwd/bwd compat.
///
/// `@sched_policy` task's scheduling policy
/// `@sched_nice` task's nice value      (`SCHED_NORMAL/BATCH`)
/// `@sched_priority` task's static priority (`SCHED_FIFO/RR`)
///
/// Certain more advanced scheduling features can be controlled by a
/// predefined set of flags via the attribute:
///
/// `@sched_flags` for customizing the scheduler behaviour
///
/// Sporadic Time-Constrained Task Attributes
/// =========================================
///
/// A subset of `sched_attr` attributes allows to describe a so-called
/// sporadic time-constrained task.
///
/// In such a model a task is specified by:
/// - the activation period or minimum instance inter-arrival time;
/// - the maximum (or average, depending on the actual scheduling
///   discipline) computation time of all instances, a.k.a. runtime;
/// - the deadline (relative to the actual activation time) of each instance.
///
/// Very briefly, a periodic (sporadic) task asks for the execution of
/// some specific computation --which is typically called an instance--
/// (at most) every period. Moreover, each instance typically lasts no more
/// than the runtime and must be completed by time instant t equal to
/// the instance activation time + the deadline.
///
/// This is reflected by the following fields of the `sched_attr` structure:
///
/// `@sched_deadline` representative of the task's deadline
/// `@sched_runtime` representative of the task's runtime
/// `@sched_period` representative of the task's period
///
/// Given this task model, there are a multiplicity of scheduling algorithms
/// and policies, that can be used to ensure all the tasks will make their
/// timing constraints.
///
/// As of now, the `SCHED_DEADLINE` policy (`sched_dl` scheduling class) is the
/// only user of this new interface. More information about the algorithm
/// available in the scheduling class file or in Documentation/.
///
/// Task Utilization Attributes
/// ===========================
///
/// A subset of `sched_attr` attributes allows to specify the utilization
/// expected for a task. These attributes allow to inform the scheduler about
/// the utilization boundaries within which it should schedule the task. These
/// boundaries are valuable hints to support scheduler decisions on both task
/// placement and frequency selection.
///
/// `@sched_util_min` represents the minimum utilization
/// `@sched_util_max` represents the maximum utilization
///
/// Utilization is a value in the range `[0..SCHED_CAPACITY_SCALE]`. It
/// represents the percentage of CPU time used by a task when running at the
/// maximum frequency on the highest capacity CPU of the system. For example, a
/// 20% utilization task is a task running for 2ms every 10ms at maximum
/// frequency.
///
/// A task with a min utilization value bigger than 0 is more likely scheduled
/// on a CPU with a capacity big enough to fit the specified value.
/// A task with a max utilization value smaller than 1024 is more likely
/// scheduled on a CPU with no more capacity than the specified value.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sched_attr_t {
    pub size: u32,

    pub sched_policy: u32,
    pub sched_flags: u64,

    /// `SCHED_NORMAL`, `SCHED_BATCH`
    pub sched_nice: i32,

    /// `SCHED_FIFO`, `SCHED_RR`
    pub sched_priority: u32,

    /// `SCHED_DEADLINE`
    pub sched_runtime: u64,
    pub sched_deadline: u64,
    pub sched_period: u64,

    /// Utilization hints
    pub sched_util_min: u32,
    pub sched_util_max: u32,
}
