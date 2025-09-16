use crate::{
    error::OciSpecError,
    runtime::{Capabilities, Capability},
};
use derive_builder::Builder;
use getset::{CopyGetters, Getters, MutGetters, Setters};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use strum_macros::{Display as StrumDisplay, EnumString};

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Deserialize,
    Getters,
    MutGetters,
    Setters,
    Eq,
    PartialEq,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// Process contains information to start a specific application inside the
/// container.
pub struct Process {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Terminal creates an interactive terminal for the container.
    terminal: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// ConsoleSize specifies the size of the console.
    console_size: Option<Box>,

    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// User specifies user information for the process.
    user: User,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Args specifies the binary and arguments for the application to
    /// execute.
    args: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// CommandLine specifies the full command line for the application to
    /// execute on Windows.
    command_line: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Env populates the process environment for the process.
    env: Option<Vec<String>>,

    #[getset(get = "pub", set = "pub")]
    /// Cwd is the current working directory for the process and must be
    /// relative to the container's root.
    cwd: PathBuf,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Capabilities are Linux capabilities that are kept for the process.
    capabilities: Option<LinuxCapabilities>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Rlimits specifies rlimit options to apply to the process.
    rlimits: Option<Vec<PosixRlimit>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// NoNewPrivileges controls whether additional privileges could be
    /// gained by processes in the container.
    no_new_privileges: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// ApparmorProfile specifies the apparmor profile for the container.
    apparmor_profile: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Specify an oom_score_adj for the container.
    oom_score_adj: Option<i32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// SelinuxLabel specifies the selinux context that the container
    /// process is run as.
    selinux_label: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// IOPriority contains the I/O priority settings for the cgroup.
    io_priority: Option<LinuxIOPriority>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Scheduler specifies the scheduling attributes for a process
    scheduler: Option<Scheduler>,

    #[serde(
        rename = "execCPUAffinity",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[getset(get = "pub", set = "pub")]
    /// ExecCPUAffinity specifies the cpu affinity for a process
    exec_cpu_affinity: Option<ExecCPUAffinity>,
}

// Default impl for processes in the container
impl Default for Process {
    fn default() -> Self {
        Process {
            // Don't create an interactive terminal for container by default
            terminal: false.into(),
            // Gives default console size of 0, 0
            console_size: Default::default(),
            // Gives process a uid and gid of 0 (root)
            user: Default::default(),
            // By default executes sh command, giving user shell
            args: vec!["sh".to_string()].into(),
            // Sets linux default environment for binaries and default xterm emulator
            env: vec![
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
                "TERM=xterm".into(),
            ]
            .into(),
            // Sets cwd of process to the container root by default
            cwd: "/".into(),
            // By default does not allow process to gain additional privileges
            no_new_privileges: true.into(),
            // Empty String, no default apparmor
            apparmor_profile: Default::default(),
            // Empty String, no default selinux
            selinux_label: Default::default(),
            // Empty String, no default scheduler
            scheduler: Default::default(),
            // See impl Default for LinuxCapabilities
            capabilities: Some(Default::default()),
            // Sets the default maximum of 1024 files the process can open
            // This is the same as the linux kernel default
            rlimits: vec![PosixRlimit {
                typ: PosixRlimitType::RlimitNofile,
                hard: 1024,
                soft: 1024,
            }]
            .into(),
            oom_score_adj: None,
            command_line: None,
            // Empty IOPriority, no default iopriority
            io_priority: Default::default(),
            exec_cpu_affinity: Default::default(),
        }
    }
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// Box specifies dimensions of a rectangle. Used for specifying the size of
/// a console.
pub struct Box {
    #[serde(default)]
    /// Height is the vertical dimension of a box.
    height: u64,

    #[serde(default)]
    /// Width is the horizontal dimension of a box.
    width: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
/// Available rlimit types (see <https://man7.org/linux/man-pages/man2/getrlimit.2.html>)
pub enum PosixRlimitType {
    /// Limit in seconds of the amount of CPU time that the process can consume.
    RlimitCpu,

    /// Maximum size in bytes of the files that the process creates.
    RlimitFsize,

    /// Maximum size of the process's data segment (init data, uninit data and
    /// heap) in bytes.
    RlimitData,

    /// Maximum size of the process stack in bytes.
    RlimitStack,

    /// Maximum size of a core dump file in bytes.
    RlimitCore,

    /// Limit on the process's resident set (the number of virtual pages
    /// resident in RAM).
    RlimitRss,

    /// Limit on number of threads for the real uid calling processes.
    RlimitNproc,

    /// One greator than the maximum number of file descriptors that one process
    /// may open.
    RlimitNofile,

    /// Maximum number of bytes of memory that may be locked into RAM.
    RlimitMemlock,

    /// Maximum size of the process's virtual memory(address space) in bytes.
    RlimitAs,

    /// Limit on the number of locks and leases for the process.
    RlimitLocks,

    /// Limit on number of signals that may be queued for the process.
    RlimitSigpending,

    /// Limit on the number of bytes that can be allocated for POSIX message
    /// queue.
    RlimitMsgqueue,

    /// Specifies a ceiling to which the process's nice value can be raised.
    RlimitNice,

    /// Specifies a ceiling on the real-time priority.
    RlimitRtprio,

    /// This is a limit (in microseconds) on the amount of CPU time that a
    /// process scheduled under a real-time scheduling policy may consume
    /// without making a blocking system call.
    RlimitRttime,
}

impl Default for PosixRlimitType {
    fn default() -> Self {
        Self::RlimitCpu
    }
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// RLimit types and restrictions.
pub struct PosixRlimit {
    #[serde(rename = "type")]
    /// Type of Rlimit to set
    typ: PosixRlimitType,

    #[serde(default)]
    /// Hard limit for specified type
    hard: u64,

    #[serde(default)]
    /// Soft limit for specified type
    soft: u64,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Getters,
    MutGetters,
    Setters,
    Eq,
    PartialEq,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// User id (uid) and group id (gid) tracks file permissions.
pub struct User {
    #[serde(default)]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// UID is the user id.
    uid: u32,

    #[serde(default)]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// GID is the group id.
    gid: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Specifies the umask of the user.
    umask: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// AdditionalGids are additional group ids set for the container's
    /// process.
    additional_gids: Option<Vec<u32>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Username is the user name.
    username: Option<String>,
}

#[derive(Builder, Clone, Debug, Deserialize, Getters, Setters, Eq, PartialEq, Serialize)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get = "pub", set = "pub")]
/// LinuxCapabilities specifies the list of allowed capabilities that are
/// kept for a process. <http://man7.org/linux/man-pages/man7/capabilities.7.html>
pub struct LinuxCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Bounding is the set of capabilities checked by the kernel.
    bounding: Option<Capabilities>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Effective is the set of capabilities checked by the kernel.
    effective: Option<Capabilities>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Inheritable is the capabilities preserved across execve.
    inheritable: Option<Capabilities>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Permitted is the limiting superset for effective capabilities.
    permitted: Option<Capabilities>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Ambient is the ambient set of capabilities that are kept.
    ambient: Option<Capabilities>,
}

// Default container's linux capabilities:
// CAP_AUDIT_WRITE gives container ability to write to linux audit logs,
// CAP_KILL gives container ability to kill non root processes
// CAP_NET_BIND_SERVICE allows container to bind to ports below 1024
impl Default for LinuxCapabilities {
    fn default() -> Self {
        let audit_write = Capability::AuditWrite;
        let cap_kill = Capability::Kill;
        let net_bind = Capability::NetBindService;
        let default_vec = vec![audit_write, cap_kill, net_bind]
            .into_iter()
            .collect::<Capabilities>();
        LinuxCapabilities {
            bounding: default_vec.clone().into(),
            effective: default_vec.clone().into(),
            inheritable: default_vec.clone().into(),
            permitted: default_vec.clone().into(),
            ambient: default_vec.into(),
        }
    }
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// RLimit types and restrictions.
pub struct LinuxIOPriority {
    #[serde(default)]
    /// Class represents an I/O scheduling class.
    class: IOPriorityClass,

    #[serde(default)]
    /// Priority for the io operation
    priority: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
/// IOPriorityClass represents an I/O scheduling class.
pub enum IOPriorityClass {
    /// This is the realtime io class. This scheduling class is given
    /// higher priority than any other in the system, processes from this class are
    /// given first access to the disk every time. Thus it needs to be used with some
    /// care, one io RT process can starve the entire system. Within the RT class,
    /// there are 8 levels of class data that determine exactly how much time this
    /// process needs the disk for on each service. In the future this might change
    /// to be more directly mappable to performance, by passing in a wanted data
    /// rate instead
    IoprioClassRt,
    /// This is the best-effort scheduling class, which is the default
    /// for any process that hasn't set a specific io priority. The class data
    /// determines how much io bandwidth the process will get, it's directly mappable
    /// to the cpu nice levels just more coarsely implemented. 0 is the highest
    /// BE prio level, 7 is the lowest. The mapping between cpu nice level and io
    /// nice level is determined as: io_nice = (cpu_nice + 20) / 5.
    IoprioClassBe,
    /// This is the idle scheduling class, processes running at this
    /// level only get io time when no one else needs the disk. The idle class has no
    /// class data, since it doesn't really apply here.
    IoprioClassIdle,
}

impl Default for IOPriorityClass {
    fn default() -> Self {
        Self::IoprioClassBe
    }
}

#[derive(Builder, Clone, Debug, Deserialize, Getters, Setters, Eq, PartialEq, Serialize)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get = "pub", set = "pub")]
/// Scheduler represents the scheduling attributes for a process. It is based on
/// the Linux sched_setattr(2) syscall.
pub struct Scheduler {
    /// Policy represents the scheduling policy (e.g., SCHED_FIFO, SCHED_RR, SCHED_OTHER).
    policy: LinuxSchedulerPolicy,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Nice is the nice value for the process, which affects its priority.
    nice: Option<i32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Priority represents the static priority of the process.
    priority: Option<i32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Flags is an array of scheduling flags.
    flags: Option<Vec<LinuxSchedulerFlag>>,

    // The following ones are used by the DEADLINE scheduler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Runtime is the amount of time in nanoseconds during which the process
    /// is allowed to run in a given period.
    runtime: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Deadline is the absolute deadline for the process to complete its execution.
    deadline: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Period is the length of the period in nanoseconds used for determining the process runtime.
    period: Option<u64>,
}

/// Default scheduler is SCHED_OTHER with no priority.
impl Default for Scheduler {
    fn default() -> Self {
        Self {
            policy: LinuxSchedulerPolicy::default(),
            nice: None,
            priority: None,
            flags: None,
            runtime: None,
            deadline: None,
            period: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
///  LinuxSchedulerPolicy represents different scheduling policies used with the Linux Scheduler
pub enum LinuxSchedulerPolicy {
    /// SchedOther is the default scheduling policy
    SchedOther,
    /// SchedFIFO is the First-In-First-Out scheduling policy
    SchedFifo,
    /// SchedRR is the Round-Robin scheduling policy
    SchedRr,
    /// SchedBatch is the Batch scheduling policy
    SchedBatch,
    /// SchedISO is the Isolation scheduling policy
    SchedIso,
    /// SchedIdle is the Idle scheduling policy
    SchedIdle,
    /// SchedDeadline is the Deadline scheduling policy
    SchedDeadline,
}

/// Default LinuxSchedulerPolicy is SchedOther
impl Default for LinuxSchedulerPolicy {
    fn default() -> Self {
        LinuxSchedulerPolicy::SchedOther
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
///  LinuxSchedulerFlag represents the flags used by the Linux Scheduler.
pub enum LinuxSchedulerFlag {
    /// SchedFlagResetOnFork represents the reset on fork scheduling flag
    SchedResetOnFork,
    /// SchedFlagReclaim represents the reclaim scheduling flag
    SchedFlagReclaim,
    /// SchedFlagDLOverrun represents the deadline overrun scheduling flag
    SchedFlagDLOverrun,
    /// SchedFlagKeepPolicy represents the keep policy scheduling flag
    SchedFlagKeepPolicy,
    /// SchedFlagKeepParams represents the keep parameters scheduling flag
    SchedFlagKeepParams,
    /// SchedFlagUtilClampMin represents the utilization clamp minimum scheduling flag
    SchedFlagUtilClampMin,
    /// SchedFlagUtilClampMin represents the utilization clamp maximum scheduling flag
    SchedFlagUtilClampMax,
}

/// Default LinuxSchedulerFlag is SchedResetOnFork
impl Default for LinuxSchedulerFlag {
    fn default() -> Self {
        LinuxSchedulerFlag::SchedResetOnFork
    }
}

#[derive(
    Builder, Clone, Debug, Default, Deserialize, Getters, Setters, Eq, PartialEq, Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(validate = "Self::validate", error = "OciSpecError")
)]
#[getset(get = "pub", set = "pub")]
/// ExecCPUAffinity specifies CPU affinity used to execute the process.
/// This setting is not applicable to the container's init process.
pub struct ExecCPUAffinity {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize"
    )]
    /// initial is a list of CPUs a runtime parent process to be run on
    /// initially, before the transition to container's cgroup.
    /// This is a a comma-separated list, with dashes to represent ranges.
    /// For example, `0-3,7` represents CPUs 0,1,2,3, and 7.
    initial: Option<String>,

    #[serde(
        default,
        rename = "final",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize"
    )]
    /// cpu_affinity_final is a list of CPUs the process will be run on after the transition
    /// to container's cgroup. The format is the same as for `initial`. If omitted or empty,
    /// runtime SHOULD NOT change process' CPU affinity after the process is moved to
    /// container's cgroup, and the final affinity is determined by the Linux kernel.
    cpu_affinity_final: Option<String>,
}

impl ExecCPUAffinityBuilder {
    fn validate(&self) -> Result<(), OciSpecError> {
        if let Some(Some(ref s)) = self.initial {
            validate_cpu_affinity(s).map_err(|e| OciSpecError::Other(e.to_string()))?;
        }

        if let Some(Some(ref s)) = self.cpu_affinity_final {
            validate_cpu_affinity(s).map_err(|e| OciSpecError::Other(e.to_string()))?;
        }

        Ok(())
    }
}

fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;

    if let Some(ref s) = value {
        validate_cpu_affinity(s).map_err(de::Error::custom)?;
    }

    Ok(value)
}

fn exec_cpu_affinity_regex() -> &'static Regex {
    static EXEC_CPU_AFFINITY_REGEX: OnceLock<Regex> = OnceLock::new();
    EXEC_CPU_AFFINITY_REGEX.get_or_init(|| {
        Regex::new(r"^(\d+(-\d+)?)(,\d+(-\d+)?)*$")
            .expect("Failed to create regex for execCPUAffinity")
    })
}

fn validate_cpu_affinity(s: &str) -> Result<(), String> {
    if !exec_cpu_affinity_regex().is_match(s) {
        return Err(format!("Invalid execCPUAffinity format: {s}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // PosixRlimitType test cases
    #[test]
    fn posix_rlimit_type_enum_to_string() {
        let type_a = PosixRlimitType::RlimitCpu;
        assert_eq!(type_a.to_string(), "RLIMIT_CPU");

        let type_b = PosixRlimitType::RlimitData;
        assert_eq!(type_b.to_string(), "RLIMIT_DATA");

        let type_c = PosixRlimitType::RlimitNofile;
        assert_eq!(type_c.to_string(), "RLIMIT_NOFILE");
    }

    #[test]
    fn posix_rlimit_type_string_to_enum() {
        let posix_rlimit_type_str = "RLIMIT_CPU";
        let posix_rlimit_type_enum: PosixRlimitType = posix_rlimit_type_str.parse().unwrap();
        assert_eq!(posix_rlimit_type_enum, PosixRlimitType::RlimitCpu);

        let posix_rlimit_type_str = "RLIMIT_DATA";
        let posix_rlimit_type_enum: PosixRlimitType = posix_rlimit_type_str.parse().unwrap();
        assert_eq!(posix_rlimit_type_enum, PosixRlimitType::RlimitData);

        let posix_rlimit_type_str = "RLIMIT_NOFILE";
        let posix_rlimit_type_enum: PosixRlimitType = posix_rlimit_type_str.parse().unwrap();
        assert_eq!(posix_rlimit_type_enum, PosixRlimitType::RlimitNofile);

        let invalid_posix_rlimit_type_str = "x";
        let unknown_rlimit = invalid_posix_rlimit_type_str.parse::<PosixRlimitType>();
        assert!(unknown_rlimit.is_err());
    }

    #[test]
    fn exec_cpu_affinity_valid_initial_final() {
        let json = json!({"initial": "0-3,7", "final": "4-6,8"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());

        let json = json!({"initial": "0-3", "final": "4-6"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());

        let json = json!({"initial": "0", "final": "4"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());
    }

    #[test]
    fn exec_cpu_affinity_invalid_initial() {
        let json = json!({"initial": "0-3,,7", "final": "4-6,8"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn exec_cpu_affinity_invalid_final() {
        let json = json!({"initial": "0-3,7", "final": "4-6.,8"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn exec_cpu_affinity_valid_final() {
        let json = json!({"final": "0,1,2,3"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());
        assert!(result.unwrap().initial.is_none());
    }

    #[test]
    fn exec_cpu_affinity_valid_initial() {
        let json = json!({"initial": "0-1,2-5"});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());
        assert!(result.unwrap().cpu_affinity_final.is_none());
    }

    #[test]
    fn exec_cpu_affinity_empty() {
        let json = json!({});
        let result: Result<ExecCPUAffinity, _> = serde_json::from_value(json);
        assert!(result.is_ok());
        let affinity = result.unwrap();
        assert!(affinity.initial.is_none());
        assert!(affinity.cpu_affinity_final.is_none());
    }

    #[test]
    fn test_build_valid_input() {
        let affinity = ExecCPUAffinityBuilder::default()
            .initial("0-3,7,8,9,10".to_string())
            .cpu_affinity_final("4-6,8".to_string())
            .build();
        assert!(affinity.is_ok());
        let affinity = affinity.unwrap();
        assert_eq!(affinity.initial, Some("0-3,7,8,9,10".to_string()));
        assert_eq!(affinity.cpu_affinity_final, Some("4-6,8".to_string()));
    }

    #[test]
    fn test_build_invalid_initial() {
        let affinity = ExecCPUAffinityBuilder::default()
            .initial("0-3,i".to_string())
            .cpu_affinity_final("4-6,8".to_string())
            .build();
        let err = affinity.unwrap_err();
        assert_eq!(err.to_string(), "Invalid execCPUAffinity format: 0-3,i");

        let affinity = ExecCPUAffinityBuilder::default()
            .initial("-".to_string())
            .cpu_affinity_final("4-6,8".to_string())
            .build();
        let err = affinity.unwrap_err();
        assert_eq!(err.to_string(), "Invalid execCPUAffinity format: -");
    }

    #[test]
    fn test_build_invalid_final() {
        let affinity = ExecCPUAffinityBuilder::default()
            .initial("0-3,7".to_string())
            .cpu_affinity_final("0-l1".to_string())
            .build();
        let err = affinity.unwrap_err();
        assert_eq!(err.to_string(), "Invalid execCPUAffinity format: 0-l1");

        let affinity = ExecCPUAffinityBuilder::default()
            .initial("0-3,7".to_string())
            .cpu_affinity_final(",1,2".to_string())
            .build();
        let err = affinity.unwrap_err();
        assert_eq!(err.to_string(), "Invalid execCPUAffinity format: ,1,2");
    }

    #[test]
    fn test_build_empty() {
        let affinity = ExecCPUAffinityBuilder::default().build();
        let affinity = affinity.unwrap();
        assert!(affinity.initial.is_none());
        assert!(affinity.cpu_affinity_final.is_none());
    }
}
