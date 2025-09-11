// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/resource.h`

use crate::timeval_t;

/// Resource limit type (low 63 bits, excluding the sign bit)
pub type rlim_t = u64;

/// Possible values of the first parameter to getpriority()/setpriority(),
/// used to indicate the type of the second parameter.
///
/// Second argument is a PID
pub const PRIO_PROCESS: i32 = 0;
/// Second argument is a GID
pub const PRIO_PGRP: i32 = 1;
/// Second argument is a UID
pub const PRIO_USER: i32 = 2;

/// Second argument is always 0 (current thread)
pub const PRIO_DARWIN_THREAD: i32 = 3;
/// Second argument is a PID
pub const PRIO_DARWIN_PROCESS: i32 = 4;

/// Range limitations for the value of the third parameter to setpriority().
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

/// use PRIO_DARWIN_BG to set the current thread into "background" state
/// which lowers CPU, disk IO, and networking priorites until thread terminates
/// or "background" state is revoked
pub const PRIO_DARWIN_BG: i32 = 0x1000;

/// use PRIO_DARWIN_NONUI to restrict a process's ability to make calls to
/// the GPU. (deprecated)
pub const PRIO_DARWIN_NONUI: i32 = 0x1001;

/// Possible values of the first parameter to getrusage(), used to indicate
/// the scope of the information to be returned.
///
/// Current process information
pub const RUSAGE_SELF: i32 = 0;
/// Current process' children
pub const RUSAGE_CHILDREN: i32 = -1;

/// A structure representing an accounting of resource utilization.
///
/// The address of an instance of this structure is the second parameter to getrusage().
///
/// Note: All values other than ru_utime and ru_stime are implementaiton
/// defined and subject to change in a future release.
/// Their use is discouraged for standards compliant programs.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_t {
    /// user time used (PL)
    pub ru_utime: timeval_t,
    /// system time used (PL)
    pub ru_stime: timeval_t,
    /// Informational aliases for source compatibility with programs
    /// that need more information than that provided by standards,
    /// and which do not mind being OS-dependent.
    ///
    /// max resident set size (PL)
    pub ru_maxrss: isize,
    /// integral shared memory size (NU)
    pub ru_ixrss: isize,
    /// integral unshared data (NU)
    pub ru_idrss: isize,
    /// integral unshared stack (NU)
    pub ru_isrss: isize,
    /// page reclaims (NU)
    pub ru_minflt: isize,
    /// page faults (NU)
    pub ru_majflt: isize,
    /// swaps (NU)
    pub ru_nswap: isize,
    /// block input operations (atomic)
    pub ru_inblock: isize,
    /// block output operations (atomic)
    pub ru_oublock: isize,
    /// messages sent (atomic)
    pub ru_msgsnd: isize,
    /// messages received (atomic)
    pub ru_msgrcv: isize,
    /// signals received (atomic)
    pub ru_nsignals: isize,
    /// voluntary context switches (atomic)
    pub ru_nvcsw: isize,
    /// involuntary
    pub ru_nivcsw: isize,
}

/// Flavors for proc_pid_rusage().
pub const RUSAGE_INFO_V0: i32 = 0;
pub const RUSAGE_INFO_V1: i32 = 1;
pub const RUSAGE_INFO_V2: i32 = 2;
pub const RUSAGE_INFO_V3: i32 = 3;
pub const RUSAGE_INFO_V4: i32 = 4;
pub const RUSAGE_INFO_V5: i32 = 5;
pub const RUSAGE_INFO_CURRENT: i32 = RUSAGE_INFO_V5;

/// Flags for RUSAGE_INFO_V5
///
/// proc has reslid shared cache
pub const RU_PROC_RUNS_RESLIDE: i32 = 0x0000_0001;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v0_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v1_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
    pub ri_child_user_time: u64,
    pub ri_child_system_time: u64,
    pub ri_child_pkg_idle_wkups: u64,
    pub ri_child_interrupt_wkups: u64,
    pub ri_child_pageins: u64,
    pub ri_child_elapsed_abstime: u64,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v2_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
    pub ri_child_user_time: u64,
    pub ri_child_system_time: u64,
    pub ri_child_pkg_idle_wkups: u64,
    pub ri_child_interrupt_wkups: u64,
    pub ri_child_pageins: u64,
    pub ri_child_elapsed_abstime: u64,
    pub ri_diskio_bytesread: u64,
    pub ri_diskio_byteswritten: u64,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v3_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
    pub ri_child_user_time: u64,
    pub ri_child_system_time: u64,
    pub ri_child_pkg_idle_wkups: u64,
    pub ri_child_interrupt_wkups: u64,
    pub ri_child_pageins: u64,
    pub ri_child_elapsed_abstime: u64,
    pub ri_diskio_bytesread: u64,
    pub ri_diskio_byteswritten: u64,
    pub ri_cpu_time_qos_default: u64,
    pub ri_cpu_time_qos_maintenance: u64,
    pub ri_cpu_time_qos_background: u64,
    pub ri_cpu_time_qos_utility: u64,
    pub ri_cpu_time_qos_legacy: u64,
    pub ri_cpu_time_qos_user_initiated: u64,
    pub ri_cpu_time_qos_user_interactive: u64,
    pub ri_billed_system_time: u64,
    pub ri_serviced_system_time: u64,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v4_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
    pub ri_child_user_time: u64,
    pub ri_child_system_time: u64,
    pub ri_child_pkg_idle_wkups: u64,
    pub ri_child_interrupt_wkups: u64,
    pub ri_child_pageins: u64,
    pub ri_child_elapsed_abstime: u64,
    pub ri_diskio_bytesread: u64,
    pub ri_diskio_byteswritten: u64,
    pub ri_cpu_time_qos_default: u64,
    pub ri_cpu_time_qos_maintenance: u64,
    pub ri_cpu_time_qos_background: u64,
    pub ri_cpu_time_qos_utility: u64,
    pub ri_cpu_time_qos_legacy: u64,
    pub ri_cpu_time_qos_user_initiated: u64,
    pub ri_cpu_time_qos_user_interactive: u64,
    pub ri_billed_system_time: u64,
    pub ri_serviced_system_time: u64,
    pub ri_logical_writes: u64,
    pub ri_lifetime_max_phys_footprint: u64,
    pub ri_instructions: u64,
    pub ri_cycles: u64,
    pub ri_billed_energy: u64,
    pub ri_serviced_energy: u64,
    pub ri_interval_max_phys_footprint: u64,
    pub ri_runnable_time: u64,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rusage_info_v5_t {
    pub ri_uuid: [u8; 16],
    pub ri_user_time: u64,
    pub ri_system_time: u64,
    pub ri_pkg_idle_wkups: u64,
    pub ri_interrupt_wkups: u64,
    pub ri_pageins: u64,
    pub ri_wired_size: u64,
    pub ri_resident_size: u64,
    pub ri_phys_footprint: u64,
    pub ri_proc_start_abstime: u64,
    pub ri_proc_exit_abstime: u64,
    pub ri_child_user_time: u64,
    pub ri_child_system_time: u64,
    pub ri_child_pkg_idle_wkups: u64,
    pub ri_child_interrupt_wkups: u64,
    pub ri_child_pageins: u64,
    pub ri_child_elapsed_abstime: u64,
    pub ri_diskio_bytesread: u64,
    pub ri_diskio_byteswritten: u64,
    pub ri_cpu_time_qos_default: u64,
    pub ri_cpu_time_qos_maintenance: u64,
    pub ri_cpu_time_qos_background: u64,
    pub ri_cpu_time_qos_utility: u64,
    pub ri_cpu_time_qos_legacy: u64,
    pub ri_cpu_time_qos_user_initiated: u64,
    pub ri_cpu_time_qos_user_interactive: u64,
    pub ri_billed_system_time: u64,
    pub ri_serviced_system_time: u64,
    pub ri_logical_writes: u64,
    pub ri_lifetime_max_phys_footprint: u64,
    pub ri_instructions: u64,
    pub ri_cycles: u64,
    pub ri_billed_energy: u64,
    pub ri_serviced_energy: u64,
    pub ri_interval_max_phys_footprint: u64,
    pub ri_runnable_time: u64,
    pub ri_flags: u64,
}

pub type rusage_info_current = rusage_info_v5_t;

/// Symbolic constants for resource limits; since all limits are representable
/// as a type rlim_t, we are permitted to define RLIM_SAVED_* in terms of
/// RLIM_INFINITY.
///
/// no limit
pub const RLIM_INFINITY: rlim_t = ((1_u64 << 63) - 1) as rlim_t;
/// Unrepresentable hard limit
pub const RLIM_SAVED_MAX: rlim_t = RLIM_INFINITY;
/// Unrepresentable soft limit
pub const RLIM_SAVED_CUR: rlim_t = RLIM_INFINITY;

/// Possible values of the first parameter to getrlimit()/setrlimit(), to
/// indicate for which resource the operation is being performed.
///
/// cpu time per process
pub const RLIMIT_CPU: i32 = 0;
/// file size
pub const RLIMIT_FSIZE: i32 = 1;
/// data segment size
pub const RLIMIT_DATA: i32 = 2;
/// stack size
pub const RLIMIT_STACK: i32 = 3;
/// core file size
pub const RLIMIT_CORE: i32 = 4;
/// address space (resident set size)
pub const RLIMIT_AS: i32 = 5;
/// source compatibility alias
pub const RLIMIT_RSS: i32 = RLIMIT_AS;
/// locked-in-memory address space
pub const RLIMIT_MEMLOCK: i32 = 6;
/// number of processes
pub const RLIMIT_NPROC: i32 = 7;
/// number of open files
pub const RLIMIT_NOFILE: i32 = 8;
/// total number of resource limits
pub const RLIM_NLIMITS: i32 = 9;
/// Set bit for strict POSIX
pub const _RLIMIT_POSIX_FLAG: i32 = 0x1000;

/// A structure representing a resource limit.  The address of an instance
/// of this structure is the second parameter to getrlimit()/setrlimit().
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rlimit_t {
    /// current (soft) limit
    pub rlim_cur: rlim_t,
    /// maximum value for rlim_cur
    pub rlim_max: rlim_t,
}

/// proc_rlimit_control()
///
/// Resource limit flavors
///
/// Configure the wakeups monitor.
pub const RLIMIT_WAKEUPS_MONITOR: i32 = 0x1;
/// Configure the CPU usage monitor.
pub const RLIMIT_CPU_USAGE_MONITOR: i32 = 0x2;
/// Configure a blocking, per-thread, CPU limits.
pub const RLIMIT_THREAD_CPULIMITS: i32 = 0x3;
/// Configure memory footprint interval tracking
pub const RLIMIT_FOOTPRINT_INTERVAL: i32 = 0x4;

/// Flags for wakeups monitor control.
pub const WAKEMON_ENABLE: i32 = 0x01;
pub const WAKEMON_DISABLE: i32 = 0x02;
pub const WAKEMON_GET_PARAMS: i32 = 0x04;
pub const WAKEMON_SET_DEFAULTS: i32 = 0x08;
/// Configure the task so that violations are fatal.
pub const WAKEMON_MAKE_FATAL: i32 = 0x10;

/// Flags for CPU usage monitor control.
pub const CPUMON_MAKE_FATAL: i32 = 0x1000;

/// Flags for memory footprint interval tracking.
///
/// Reset the footprint interval counter to zero
pub const FOOTPRINT_INTERVAL_RESET: i32 = 0x1;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct proc_rlimit_control_wakeupmon_t {
    pub wm_flags: u32,
    pub wm_rate: i32,
}

/// I/O type
pub const IOPOL_TYPE_DISK: i32 = 0;
pub const IOPOL_TYPE_VFS_ATIME_UPDATES: i32 = 2;
pub const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: i32 = 3;
pub const IOPOL_TYPE_VFS_STATFS_NO_DATA_VOLUME: i32 = 4;
pub const IOPOL_TYPE_VFS_TRIGGER_RESOLVE: i32 = 5;
pub const IOPOL_TYPE_VFS_IGNORE_CONTENT_PROTECTION: i32 = 6;

/// scope
pub const IOPOL_SCOPE_PROCESS: i32 = 0;
pub const IOPOL_SCOPE_THREAD: i32 = 1;
pub const IOPOL_SCOPE_DARWIN_BG: i32 = 2;

/// I/O Priority
pub const IOPOL_DEFAULT: i32 = 0;
pub const IOPOL_IMPORTANT: i32 = 1;
pub const IOPOL_PASSIVE: i32 = 2;
pub const IOPOL_THROTTLE: i32 = 3;
pub const IOPOL_UTILITY: i32 = 4;
pub const IOPOL_STANDARD: i32 = 5;

/// compatibility with older names
pub const IOPOL_APPLICATION: i32 = IOPOL_STANDARD;
pub const IOPOL_NORMAL: i32 = IOPOL_IMPORTANT;

pub const IOPOL_ATIME_UPDATES_DEFAULT: i32 = 0;
pub const IOPOL_ATIME_UPDATES_OFF: i32 = 1;

pub const IOPOL_MATERIALIZE_DATALESS_FILES_DEFAULT: i32 = 0;
pub const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: i32 = 1;
pub const IOPOL_MATERIALIZE_DATALESS_FILES_ON: i32 = 2;

pub const IOPOL_VFS_STATFS_NO_DATA_VOLUME_DEFAULT: i32 = 0;
pub const IOPOL_VFS_STATFS_FORCE_NO_DATA_VOLUME: i32 = 1;

pub const IOPOL_VFS_TRIGGER_RESOLVE_DEFAULT: i32 = 0;
pub const IOPOL_VFS_TRIGGER_RESOLVE_OFF: i32 = 1;

pub const IOPOL_VFS_CONTENT_PROTECTION_DEFAULT: i32 = 0;
pub const IOPOL_VFS_CONTENT_PROTECTION_IGNORE: i32 = 1;
