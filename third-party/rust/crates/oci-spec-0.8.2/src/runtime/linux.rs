use crate::error::{oci_error, OciSpecError};

use derive_builder::Builder;
use getset::{CopyGetters, Getters, MutGetters, Setters};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display, path::PathBuf, vec};
use strum_macros::{Display as StrumDisplay, EnumString};

#[derive(
    Builder, Clone, Debug, Deserialize, Eq, Getters, MutGetters, Setters, PartialEq, Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_mut = "pub", get = "pub", set = "pub")]
/// Linux contains platform-specific configuration for Linux based
/// containers.
pub struct Linux {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// NetDevices are key-value pairs, keyed by network device name on the host, moved to the container's network namespace.
    net_devices: Option<HashMap<String, LinuxNetDevice>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// UIDMappings specifies user mappings for supporting user namespaces.
    uid_mappings: Option<Vec<LinuxIdMapping>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// GIDMappings specifies group mappings for supporting user namespaces.
    gid_mappings: Option<Vec<LinuxIdMapping>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Sysctl are a set of key value pairs that are set for the container
    /// on start.
    sysctl: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Resources contain cgroup information for handling resource
    /// constraints for the container.
    resources: Option<LinuxResources>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// CgroupsPath specifies the path to cgroups that are created and/or
    /// joined by the container. The path is expected to be relative
    /// to the cgroups mountpoint. If resources are specified,
    /// the cgroups at CgroupsPath will be updated based on resources.
    cgroups_path: Option<PathBuf>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Namespaces contains the namespaces that are created and/or joined by
    /// the container.
    namespaces: Option<Vec<LinuxNamespace>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Devices are a list of device nodes that are created for the
    /// container.
    devices: Option<Vec<LinuxDevice>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Seccomp specifies the seccomp security settings for the container.
    seccomp: Option<LinuxSeccomp>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RootfsPropagation is the rootfs mount propagation mode for the
    /// container.
    rootfs_propagation: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// MaskedPaths masks over the provided paths inside the container.
    masked_paths: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// ReadonlyPaths sets the provided paths as RO inside the container.
    readonly_paths: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// MountLabel specifies the selinux context for the mounts in the
    /// container.
    mount_label: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// IntelRdt contains Intel Resource Director Technology (RDT)
    /// information for handling resource constraints and monitoring metrics
    /// (e.g., L3 cache, memory bandwidth) for the container.
    intel_rdt: Option<LinuxIntelRdt>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Personality contains configuration for the Linux personality
    /// syscall.
    personality: Option<LinuxPersonality>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// TimeOffsets specifies the offset for supporting time namespaces.
    time_offsets: Option<HashMap<String, LinuxTimeOffset>>,
}

// Default impl for Linux (see functions for more info)
impl Default for Linux {
    fn default() -> Self {
        Linux {
            // Creates empty Hashmap
            net_devices: None,
            // Creates empty Vec
            uid_mappings: Default::default(),
            // Creates empty Vec
            gid_mappings: Default::default(),
            // Empty sysctl Hashmap
            sysctl: Default::default(),
            resources: Some(LinuxResources {
                devices: vec![].into(),
                memory: Default::default(),
                cpu: Default::default(),
                pids: Default::default(),
                block_io: Default::default(),
                hugepage_limits: Default::default(),
                network: Default::default(),
                rdma: Default::default(),
                unified: Default::default(),
            }),
            // Defaults to None
            cgroups_path: Default::default(),
            namespaces: get_default_namespaces().into(),
            // Empty Vec
            devices: Default::default(),
            // Empty String
            rootfs_propagation: Default::default(),
            masked_paths: get_default_maskedpaths().into(),
            readonly_paths: get_default_readonly_paths().into(),
            // Empty String
            mount_label: Default::default(),
            seccomp: None,
            intel_rdt: None,
            personality: None,
            time_offsets: None,
        }
    }
}

impl Linux {
    /// Return rootless Linux configuration.
    pub fn rootless(uid: u32, gid: u32) -> Self {
        let mut namespaces = get_default_namespaces();
        namespaces.retain(|ns| ns.typ != LinuxNamespaceType::Network);
        namespaces.push(LinuxNamespace {
            typ: LinuxNamespaceType::User,
            ..Default::default()
        });
        Self {
            resources: None,
            uid_mappings: Some(vec![LinuxIdMapping {
                container_id: 0,
                host_id: uid,
                size: 1,
            }]),
            gid_mappings: Some(vec![LinuxIdMapping {
                container_id: 0,
                host_id: gid,
                size: 1,
            }]),
            namespaces: Some(namespaces),
            ..Default::default()
        }
    }
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// LinuxIDMapping specifies UID/GID mappings.
pub struct LinuxIdMapping {
    #[serde(default, rename = "hostID")]
    /// HostID is the starting UID/GID on the host to be mapped to
    /// `container_id`.
    host_id: u32,

    #[serde(default, rename = "containerID")]
    /// ContainerID is the starting UID/GID in the container.
    container_id: u32,

    #[serde(default)]
    /// Size is the number of IDs to be mapped.
    size: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, EnumString)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
/// Device types
pub enum LinuxDeviceType {
    /// All
    A,

    /// block (buffered)
    B,

    /// character (unbuffered)
    C,

    /// character (unbufferd)
    U,

    /// FIFO
    P,
}

#[allow(clippy::derivable_impls)] // because making it clear that All is the default
impl Default for LinuxDeviceType {
    fn default() -> LinuxDeviceType {
        LinuxDeviceType::A
    }
}

impl LinuxDeviceType {
    /// Retrieve a string reference for the device type.
    pub fn as_str(&self) -> &str {
        match self {
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
            Self::U => "u",
            Self::P => "p",
        }
    }
}

#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    MutGetters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxNetDevice represents a single network device to be added to the container's network namespace
pub struct LinuxNetDevice {
    #[serde(default)]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Name of the device in the container namespace
    name: Option<String>,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    MutGetters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// Represents a device rule for the devices specified to the device
/// controller
pub struct LinuxDeviceCgroup {
    #[serde(default)]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Allow or deny
    allow: bool,

    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Device type, block, char, etc.
    typ: Option<LinuxDeviceType>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Device's major number
    major: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Device's minor number
    minor: Option<i64>,

    /// Cgroup access permissions format, rwm.
    #[serde(default)]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    access: Option<String>,
}

/// This ToString trait is automatically implemented for any type which implements the Display trait.
/// As such, ToString shouldn’t be implemented directly: Display should be implemented instead,
/// and you get the ToString implementation for free.
impl Display for LinuxDeviceCgroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let major = self
            .major
            .map(|mj| mj.to_string())
            .unwrap_or_else(|| "*".to_string());
        let minor = self
            .minor
            .map(|mi| mi.to_string())
            .unwrap_or_else(|| "*".to_string());
        let access = self.access.as_deref().unwrap_or("");
        write!(
            f,
            "{} {}:{} {}",
            &self.typ.unwrap_or_default().as_str(),
            &major,
            &minor,
            &access
        )
    }
}

#[derive(
    Builder,
    Clone,
    Copy,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    PartialEq,
    Serialize,
    Setters,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// LinuxMemory for Linux cgroup 'memory' resource management.
pub struct LinuxMemory {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Memory limit (in bytes).
    limit: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Memory reservation or soft_limit (in bytes).
    reservation: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Total memory limit (memory + swap).
    swap: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Kernel memory limit (in bytes).
    kernel: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "kernelTCP")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Kernel memory limit for tcp (in bytes).
    kernel_tcp: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// How aggressive the kernel will swap memory pages.
    swappiness: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "disableOOMKiller")]
    #[getset(get_copy = "pub", set = "pub")]
    /// DisableOOMKiller disables the OOM killer for out of memory
    /// conditions.
    disable_oom_killer: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Enables hierarchical memory accounting
    use_hierarchy: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Enables checking if a new memory limit is lower
    check_before_update: Option<bool>,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
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
/// LinuxCPU for Linux cgroup 'cpu' resource management.
pub struct LinuxCpu {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// CPU shares (relative weight (ratio) vs. other cgroups with cpu
    /// shares).
    shares: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// CPU hardcap limit (in usecs). Allowed cpu time in a given period.
    quota: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Cgroups are configured with minimum weight, 0: default behavior, 1: SCHED_IDLE.
    idle: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Maximum amount of accumulated time in microseconds for which tasks
    /// in a cgroup can run additionally for burst during one period
    burst: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// CPU period to be used for hardcapping (in usecs).
    period: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// How much time realtime scheduling may use (in usecs).
    realtime_runtime: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// CPU period to be used for realtime scheduling (in usecs).
    realtime_period: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// CPUs to use within the cpuset. Default is to use any CPU available.
    cpus: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// List of memory nodes in the cpuset. Default is to use any available
    /// memory node.
    mems: Option<String>,
}

#[derive(
    Builder,
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    CopyGetters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// LinuxPids for Linux cgroup 'pids' resource management (Linux 4.3).
pub struct LinuxPids {
    #[serde(default)]
    /// Maximum number of PIDs. Default is "no limit".
    limit: i64,
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// LinuxWeightDevice struct holds a `major:minor weight` pair for
/// weightDevice.
pub struct LinuxWeightDevice {
    #[serde(default)]
    /// Major is the device's major number.
    major: i64,

    #[serde(default)]
    /// Minor is the device's minor number.
    minor: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// Weight is the bandwidth rate for the device.
    weight: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// LeafWeight is the bandwidth rate for the device while competing with
    /// the cgroup's child cgroups, CFQ scheduler only.
    leaf_weight: Option<u16>,
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
/// LinuxThrottleDevice struct holds a `major:minor rate_per_second` pair.
pub struct LinuxThrottleDevice {
    #[serde(default)]
    /// Major is the device's major number.
    major: i64,

    #[serde(default)]
    /// Minor is the device's minor number.
    minor: i64,

    #[serde(default)]
    /// Rate is the IO rate limit per cgroup per device.
    rate: u64,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
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
/// LinuxBlockIO for Linux cgroup 'blkio' resource management.
pub struct LinuxBlockIo {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Specifies per cgroup weight.
    weight: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Specifies tasks' weight in the given cgroup while competing with the
    /// cgroup's child cgroups, CFQ scheduler only.
    leaf_weight: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Weight per cgroup per device, can override BlkioWeight.
    weight_device: Option<Vec<LinuxWeightDevice>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// IO read rate limit per cgroup per device, bytes per second.
    throttle_read_bps_device: Option<Vec<LinuxThrottleDevice>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// IO write rate limit per cgroup per device, bytes per second.
    throttle_write_bps_device: Option<Vec<LinuxThrottleDevice>>,

    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "throttleReadIOPSDevice"
    )]
    #[getset(get = "pub", set = "pub")]
    /// IO read rate limit per cgroup per device, IO per second.
    throttle_read_iops_device: Option<Vec<LinuxThrottleDevice>>,

    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "throttleWriteIOPSDevice"
    )]
    #[getset(get = "pub", set = "pub")]
    /// IO write rate limit per cgroup per device, IO per second.
    throttle_write_iops_device: Option<Vec<LinuxThrottleDevice>>,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
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
/// LinuxHugepageLimit structure corresponds to limiting kernel hugepages.
/// Default to reservation limits if supported. Otherwise fallback to page fault limits.
pub struct LinuxHugepageLimit {
    #[serde(default)]
    #[getset(get = "pub", set = "pub")]
    /// Pagesize is the hugepage size.
    /// Format: "&lt;size&gt;&lt;unit-prefix&gt;B' (e.g. 64KB, 2MB, 1GB, etc.)
    page_size: String,

    #[serde(default)]
    #[getset(get_copy = "pub", set = "pub")]
    /// Limit is the limit of "hugepagesize" hugetlb reservations (if supported) or usage.
    limit: i64,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxInterfacePriority for network interfaces.
pub struct LinuxInterfacePriority {
    #[serde(default)]
    #[getset(get = "pub", set = "pub")]
    /// Name is the name of the network interface.
    name: String,

    #[serde(default)]
    #[getset(get_copy = "pub", set = "pub")]
    /// Priority for the interface.
    priority: u32,
}

/// This ToString trait is automatically implemented for any type which implements the Display trait.
/// As such, ToString shouldn’t be implemented directly: Display should be implemented instead,
/// and you get the ToString implementation for free.
impl Display for LinuxInterfacePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Serde serialization never fails since this is
        // a combination of String and enums.
        writeln!(f, "{} {}", self.name, self.priority)
    }
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxNetwork identification and priority configuration.
pub struct LinuxNetwork {
    #[serde(skip_serializing_if = "Option::is_none", rename = "classID")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Set class identifier for container's network packets
    class_id: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Set priority of network traffic for container.
    priorities: Option<Vec<LinuxInterfacePriority>>,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    MutGetters,
    Setters,
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
/// Resource constraints for container
pub struct LinuxResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Devices configures the device allowlist.
    devices: Option<Vec<LinuxDeviceCgroup>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Memory restriction configuration.
    memory: Option<LinuxMemory>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// CPU resource restriction configuration.
    cpu: Option<LinuxCpu>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Task resource restrictions
    pids: Option<LinuxPids>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "blockIO")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// BlockIO restriction configuration.
    block_io: Option<LinuxBlockIo>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Hugetlb limit (in bytes).
    hugepage_limits: Option<Vec<LinuxHugepageLimit>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Network restriction configuration.
    network: Option<LinuxNetwork>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Rdma resource restriction configuration. Limits are a set of key
    /// value pairs that define RDMA resource limits, where the key
    /// is device name and value is resource limits.
    rdma: Option<HashMap<String, LinuxRdma>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Unified resources.
    unified: Option<HashMap<String, String>>,
}

#[derive(
    Builder,
    Clone,
    Copy,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
/// LinuxRdma for Linux cgroup 'rdma' resource management (Linux 4.11).
pub struct LinuxRdma {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Maximum number of HCA handles that can be opened. Default is "no
    /// limit".
    hca_handles: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// Maximum number of HCA objects that can be created. Default is "no
    /// limit".
    hca_objects: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, StrumDisplay)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "snake_case")]
/// Available Linux namespaces.
pub enum LinuxNamespaceType {
    #[strum(to_string = "mnt")]
    /// Mount Namespace for isolating mount points
    Mount = 0x00020000,

    /// Cgroup Namespace for isolating cgroup hierarchies
    Cgroup = 0x02000000,

    /// Uts Namespace for isolating hostname and NIS domain name
    Uts = 0x04000000,

    /// Ipc Namespace for isolating System V, IPC, POSIX message queues
    Ipc = 0x08000000,

    /// User Namespace for isolating user and group  ids
    User = 0x10000000,

    /// PID Namespace for isolating process ids
    Pid = 0x20000000,

    #[strum(to_string = "net")]
    /// Network Namespace for isolating network devices, ports, stacks etc.
    Network = 0x40000000,

    /// Time Namespace for isolating the clocks
    Time = 0x00000080,
}

impl TryFrom<&str> for LinuxNamespaceType {
    type Error = OciSpecError;

    fn try_from(namespace: &str) -> Result<Self, Self::Error> {
        match namespace {
            "mnt" | "mount" => Ok(LinuxNamespaceType::Mount),
            "cgroup" => Ok(LinuxNamespaceType::Cgroup),
            "uts" => Ok(LinuxNamespaceType::Uts),
            "ipc" => Ok(LinuxNamespaceType::Ipc),
            "user" => Ok(LinuxNamespaceType::User),
            "pid" => Ok(LinuxNamespaceType::Pid),
            "net" | "network" => Ok(LinuxNamespaceType::Network),
            "time" => Ok(LinuxNamespaceType::Time),
            _ => Err(oci_error(format!(
                "unknown namespace {namespace}, could not convert"
            ))),
        }
    }
}

impl Default for LinuxNamespaceType {
    fn default() -> Self {
        Self::Pid
    }
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxNamespace is the configuration for a Linux namespace.
pub struct LinuxNamespace {
    #[serde(rename = "type")]
    #[getset(get_copy = "pub", set = "pub")]
    /// Type is the type of namespace.
    typ: LinuxNamespaceType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Path is a path to an existing namespace persisted on disk that can
    /// be joined and is of the same type
    path: Option<PathBuf>,
}

/// Utility function to get default namespaces.
pub fn get_default_namespaces() -> Vec<LinuxNamespace> {
    vec![
        LinuxNamespace {
            typ: LinuxNamespaceType::Pid,
            path: Default::default(),
        },
        LinuxNamespace {
            typ: LinuxNamespaceType::Network,
            path: Default::default(),
        },
        LinuxNamespace {
            typ: LinuxNamespaceType::Ipc,
            path: Default::default(),
        },
        LinuxNamespace {
            typ: LinuxNamespaceType::Uts,
            path: Default::default(),
        },
        LinuxNamespace {
            typ: LinuxNamespaceType::Mount,
            path: Default::default(),
        },
        LinuxNamespace {
            typ: LinuxNamespaceType::Cgroup,
            path: Default::default(),
        },
    ]
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    MutGetters,
    Setters,
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
/// LinuxDevice represents the mknod information for a Linux special device
/// file.
pub struct LinuxDevice {
    #[serde(default)]
    #[getset(get_mut = "pub", get = "pub", set = "pub")]
    /// Path to the device.
    path: PathBuf,

    #[serde(rename = "type")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Device type, block, char, etc..
    typ: LinuxDeviceType,

    #[serde(default)]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Major is the device's major number.
    major: i64,

    #[serde(default)]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Minor is the device's minor number.
    minor: i64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// FileMode permission bits for the device.
    file_mode: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// UID of the device.
    uid: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_mut = "pub", get_copy = "pub", set = "pub")]
    /// Gid of the device.
    gid: Option<u32>,
}

impl From<&LinuxDevice> for LinuxDeviceCgroup {
    fn from(linux_device: &LinuxDevice) -> LinuxDeviceCgroup {
        LinuxDeviceCgroup {
            allow: true,
            typ: linux_device.typ.into(),
            major: Some(linux_device.major),
            minor: Some(linux_device.minor),
            access: "rwm".to_string().into(),
        }
    }
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
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
/// LinuxSeccomp represents syscall restrictions.
pub struct LinuxSeccomp {
    #[getset(get_copy = "pub", set = "pub")]
    /// The default action to be done.
    default_action: LinuxSeccompAction,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// The default error return code to use when the default action is SCMP_ACT_ERRNO.
    default_errno_ret: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Available architectures for the restriction.
    architectures: Option<Vec<Arch>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Flags added to the seccomp restriction.
    flags: Option<Vec<LinuxSeccompFilterFlag>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// The unix domain socket path over which runtime will use for `SCMP_ACT_NOTIFY`.
    listener_path: Option<PathBuf>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// An opaque data to pass to the seccomp agent.
    listener_metadata: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// The syscalls for the restriction.
    syscalls: Option<Vec<LinuxSyscall>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
/// Available seccomp actions.
pub enum LinuxSeccompAction {
    /// Kill the thread, defined for backward compatibility.
    ScmpActKill,

    /// Kill the thread
    ScmpActKillThread,

    /// Kill the process.
    ScmpActKillProcess,

    /// Throw a SIGSYS signal.
    ScmpActTrap,

    /// Return the specified error code.
    ScmpActErrno,

    /// Notifies userspace.
    ScmpActNotify,

    /// Notify a tracing process with the specified value.
    ScmpActTrace,

    /// Allow the syscall to be executed after the action has been logged.
    ScmpActLog,

    /// Allow the syscall to be executed.
    ScmpActAllow,
}

impl From<LinuxSeccompAction> for u32 {
    fn from(action: LinuxSeccompAction) -> Self {
        match action {
            LinuxSeccompAction::ScmpActKill => 0x00000000,
            LinuxSeccompAction::ScmpActKillThread => 0x00000000,
            LinuxSeccompAction::ScmpActKillProcess => 0x80000000,
            LinuxSeccompAction::ScmpActTrap => 0x00030000,
            LinuxSeccompAction::ScmpActErrno => 0x00050001,
            LinuxSeccompAction::ScmpActNotify => 0x7fc00000,
            LinuxSeccompAction::ScmpActTrace => 0x7ff00001,
            LinuxSeccompAction::ScmpActLog => 0x7ffc0000,
            LinuxSeccompAction::ScmpActAllow => 0x7fff0000,
        }
    }
}

impl Default for LinuxSeccompAction {
    fn default() -> Self {
        Self::ScmpActAllow
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u32)]
/// Available seccomp architectures.
pub enum Arch {
    /// The native architecture.
    ScmpArchNative = 0x00000000,

    /// The x86 (32-bit) architecture.
    ScmpArchX86 = 0x40000003,

    /// The x86-64 (64-bit) architecture.
    ScmpArchX86_64 = 0xc000003e,

    /// The x32 (32-bit x86_64) architecture.
    ///
    /// This is different from the value used by the kernel because we need to
    /// be able to distinguish between x32 and x86_64.
    ScmpArchX32 = 0x4000003e,

    /// The ARM architecture.
    ScmpArchArm = 0x40000028,

    /// The AArch64 architecture.
    ScmpArchAarch64 = 0xc00000b7,

    /// The MIPS architecture.
    ScmpArchMips = 0x00000008,

    /// The MIPS64 architecture.
    ScmpArchMips64 = 0x80000008,

    /// The MIPS64n32 architecture.
    ScmpArchMips64n32 = 0xa0000008,

    /// The MIPSel architecture.
    ScmpArchMipsel = 0x40000008,

    /// The MIPSel64 architecture.
    ScmpArchMipsel64 = 0xc0000008,

    /// The MIPSel64n32 architecture.
    ScmpArchMipsel64n32 = 0xe0000008,

    /// The PowerPC architecture.
    ScmpArchPpc = 0x00000014,

    /// The PowerPC64 architecture.
    ScmpArchPpc64 = 0x80000015,

    /// The PowerPC64le architecture.
    ScmpArchPpc64le = 0xc0000015,

    /// The S390 architecture.
    ScmpArchS390 = 0x00000016,

    /// The S390x architecture.
    ScmpArchS390x = 0x80000016,

    /// The RISCV64 architecture.
    ScmpArchRiscv64 = 0xc00000f3,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
/// Available seccomp filter flags.
pub enum LinuxSeccompFilterFlag {
    /// All filter return actions except SECCOMP_RET_ALLOW should be logged. An administrator may
    /// override this filter flag by preventing specific actions from being logged via the
    /// /proc/sys/kernel/seccomp/actions_logged file. (since Linux 4.14)
    SeccompFilterFlagLog,

    /// When adding a new filter, synchronize all other threads of the calling process to the same
    /// seccomp filter tree. A "filter tree" is the ordered list of filters attached to a thread.
    /// (Attaching identical filters in separate seccomp() calls results in different filters from this
    /// perspective.)
    ///
    /// If any thread cannot synchronize to the same filter tree, the call will not attach the new
    /// seccomp filter, and will fail, returning the first thread ID found that cannot synchronize.
    /// Synchronization will fail if another thread in the same process is in SECCOMP_MODE_STRICT or if
    /// it has attached new seccomp filters to itself, diverging from the calling thread's filter tree.
    SeccompFilterFlagTsync,

    /// Disable Speculative Store Bypass mitigation. (since Linux 4.17)
    SeccompFilterFlagSpecAllow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u32)]
/// The seccomp operator to be used for args.
pub enum LinuxSeccompOperator {
    /// Refers to the SCMP_CMP_NE operator (not equal).
    ScmpCmpNe = 1,

    /// Refers to the SCMP_CMP_LT operator (less than).
    ScmpCmpLt = 2,

    /// Refers to the SCMP_CMP_LE operator (less equal).
    ScmpCmpLe = 3,

    /// Refers to the SCMP_CMP_EQ operator (equal to).
    ScmpCmpEq = 4,

    /// Refers to the SCMP_CMP_GE operator (greater equal).
    ScmpCmpGe = 5,

    /// Refers to the SCMP_CMP_GT operator (greater than).
    ScmpCmpGt = 6,

    /// Refers to the SCMP_CMP_MASKED_EQ operator (masked equal).
    ScmpCmpMaskedEq = 7,
}

impl Default for LinuxSeccompOperator {
    fn default() -> Self {
        Self::ScmpCmpEq
    }
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
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
/// LinuxSyscall is used to match a syscall in seccomp.
pub struct LinuxSyscall {
    #[getset(get = "pub", set = "pub")]
    /// The names of the syscalls.
    names: Vec<String>,

    #[getset(get_copy = "pub", set = "pub")]
    /// The action to be done for the syscalls.
    action: LinuxSeccompAction,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    /// The error return value.
    errno_ret: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// The arguments for the syscalls.
    args: Option<Vec<LinuxSeccompArg>>,
}

#[derive(
    Builder, Clone, Copy, CopyGetters, Debug, Default, Deserialize, Eq, PartialEq, Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get_copy = "pub", set = "pub")]
/// LinuxSeccompArg used for matching specific syscall arguments in seccomp.
pub struct LinuxSeccompArg {
    /// The index of the argument.
    index: usize,

    /// The value of the argument.
    value: u64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The second value of the argument.
    value_two: Option<u64>,

    /// The operator for the argument.
    op: LinuxSeccompOperator,
}

/// Default masks paths, cannot read these host files.
pub fn get_default_maskedpaths() -> Vec<String> {
    vec![
        // For example now host interfaces such as
        // bluetooth cannot be accessed due to /proc/acpi
        "/proc/acpi".to_string(),
        "/proc/asound".to_string(),
        "/proc/kcore".to_string(),
        "/proc/keys".to_string(),
        "/proc/latency_stats".to_string(),
        "/proc/timer_list".to_string(),
        "/proc/timer_stats".to_string(),
        "/proc/sched_debug".to_string(),
        "/sys/firmware".to_string(),
        "/proc/scsi".to_string(),
    ]
}

/// Default readonly paths, for example most containers shouldn't have permission to write to
/// `/proc/sys`.
pub fn get_default_readonly_paths() -> Vec<String> {
    vec![
        "/proc/bus".to_string(),
        "/proc/fs".to_string(),
        "/proc/irq".to_string(),
        "/proc/sys".to_string(),
        "/proc/sysrq-trigger".to_string(),
    ]
}

#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    MutGetters,
    Setters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
/// LinuxIntelRdt has container runtime resource constraints for Intel RDT CAT and MBA
/// features and flags enabling Intel RDT CMT and MBM features.
/// Intel RDT features are available in Linux 4.14 and newer kernel versions.
pub struct LinuxIntelRdt {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "closID")]
    /// The identity for RDT Class of Service.
    clos_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The schema for L3 cache id and capacity bitmask (CBM).
    /// Format: "L3:&lt;cache_id0&gt;=&lt;cbm0&gt;;&lt;cache_id1&gt;=&lt;cbm1&gt;;..."
    l3_cache_schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The schema of memory bandwidth per L3 cache id.
    /// Format: "MB:&lt;cache_id0&gt;=bandwidth0;&lt;cache_id1&gt;=bandwidth1;..."
    /// The unit of memory bandwidth is specified in "percentages" by
    /// default, and in "MBps" if MBA Software Controller is
    /// enabled.
    mem_bw_schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// EnableCMT is the flag to indicate if the Intel RDT CMT is enabled. CMT (Cache Monitoring Technology) supports monitoring of
    /// the last-level cache (LLC) occupancy for the container.
    enable_cmt: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// EnableMBM is the flag to indicate if the Intel RDT MBM is enabled. MBM (Memory Bandwidth Monitoring) supports monitoring of
    /// total and local memory bandwidth for the container.
    enable_mbm: Option<bool>,
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxPersonality represents the Linux personality syscall input.
pub struct LinuxPersonality {
    #[getset(get_copy = "pub", set = "pub")]
    /// Domain for the personality.
    domain: LinuxPersonalityDomain,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    /// Additional flags
    flags: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, StrumDisplay, EnumString)]
/// Define domain and flags for LinuxPersonality.
pub enum LinuxPersonalityDomain {
    #[serde(rename = "LINUX")]
    #[strum(serialize = "LINUX")]
    /// PerLinux is the standard Linux personality.
    PerLinux,

    #[serde(rename = "LINUX32")]
    #[strum(serialize = "LINUX32")]
    /// PerLinux32 sets personality to 32 bit.
    PerLinux32,
}

impl Default for LinuxPersonalityDomain {
    fn default() -> Self {
        Self::PerLinux
    }
}

#[derive(
    Builder,
    Clone,
    CopyGetters,
    Debug,
    Default,
    Deserialize,
    Eq,
    Getters,
    Setters,
    PartialEq,
    Serialize,
)]
#[builder(
    default,
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// LinuxTimeOffset specifies the offset for Time Namespace
pub struct LinuxTimeOffset {
    /// Secs is the offset of clock (in secs) in the container
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    secs: Option<i64>,
    /// Nanosecs is the additional offset for Secs (in nanosecs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[getset(get_copy = "pub", set = "pub")]
    nanosecs: Option<u32>,
}

#[cfg(feature = "proptests")]
use quickcheck::{Arbitrary, Gen};

#[cfg(feature = "proptests")]
fn some_none_generator_util<T: Arbitrary>(g: &mut Gen) -> Option<T> {
    let choice = g.choose(&[true, false]).unwrap();
    match choice {
        false => None,
        true => Some(T::arbitrary(g)),
    }
}

#[cfg(feature = "proptests")]
impl Arbitrary for LinuxDeviceCgroup {
    fn arbitrary(g: &mut Gen) -> LinuxDeviceCgroup {
        let typ_choices = ["a", "b", "c", "u", "p"];

        let typ_chosen = g.choose(&typ_choices).unwrap();

        let typ = match typ_chosen.to_string().as_str() {
            "a" => LinuxDeviceType::A,
            "b" => LinuxDeviceType::B,
            "c" => LinuxDeviceType::C,
            "u" => LinuxDeviceType::U,
            "p" => LinuxDeviceType::P,
            _ => LinuxDeviceType::A,
        };

        let access_choices = ["rwm", "m"];
        LinuxDeviceCgroup {
            allow: bool::arbitrary(g),
            typ: typ.into(),
            major: some_none_generator_util::<i64>(g),
            minor: some_none_generator_util::<i64>(g),
            access: g.choose(&access_choices).unwrap().to_string().into(),
        }
    }
}

#[cfg(feature = "proptests")]
impl Arbitrary for LinuxMemory {
    fn arbitrary(g: &mut Gen) -> LinuxMemory {
        LinuxMemory {
            kernel: some_none_generator_util::<i64>(g),
            kernel_tcp: some_none_generator_util::<i64>(g),
            limit: some_none_generator_util::<i64>(g),
            reservation: some_none_generator_util::<i64>(g),
            swap: some_none_generator_util::<i64>(g),
            swappiness: some_none_generator_util::<u64>(g),
            disable_oom_killer: some_none_generator_util::<bool>(g),
            use_hierarchy: some_none_generator_util::<bool>(g),
            check_before_update: some_none_generator_util::<bool>(g),
        }
    }
}

#[cfg(feature = "proptests")]
impl Arbitrary for LinuxHugepageLimit {
    fn arbitrary(g: &mut Gen) -> LinuxHugepageLimit {
        let unit_choice = ["KB", "MB", "GB"];
        let unit = g.choose(&unit_choice).unwrap();
        let page_size = u64::arbitrary(g).to_string() + unit;

        LinuxHugepageLimit {
            page_size,
            limit: i64::arbitrary(g),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // LinuxDeviceType test cases
    #[test]
    fn device_type_enum_to_str() {
        let type_a = LinuxDeviceType::A;
        assert_eq!(type_a.as_str(), "a");

        let type_b = LinuxDeviceType::B;
        assert_eq!(type_b.as_str(), "b");

        let type_c = LinuxDeviceType::C;
        assert_eq!(type_c.as_str(), "c");
    }

    #[test]
    fn device_type_string_to_enum() {
        let devtype_str = "a";
        let devtype_enum: LinuxDeviceType = devtype_str.parse().unwrap();
        assert_eq!(devtype_enum, LinuxDeviceType::A);

        let devtype_str = "b";
        let devtype_enum: LinuxDeviceType = devtype_str.parse().unwrap();
        assert_eq!(devtype_enum, LinuxDeviceType::B);

        let devtype_str = "c";
        let devtype_enum: LinuxDeviceType = devtype_str.parse().unwrap();
        assert_eq!(devtype_enum, LinuxDeviceType::C);

        let invalid_devtype_str = "x";
        let unknown_devtype = invalid_devtype_str.parse::<LinuxDeviceType>();
        assert!(unknown_devtype.is_err());
    }

    // LinuxNamespaceType test cases
    #[test]
    fn ns_type_enum_to_string() {
        let type_a = LinuxNamespaceType::Network;
        assert_eq!(type_a.to_string(), "net");

        let type_b = LinuxNamespaceType::Mount;
        assert_eq!(type_b.to_string(), "mnt");

        let type_c = LinuxNamespaceType::Ipc;
        assert_eq!(type_c.to_string(), "ipc");
    }

    #[test]
    fn ns_type_string_to_enum() {
        let nstype_str = "net";
        let nstype_enum = LinuxNamespaceType::try_from(nstype_str).unwrap();
        assert_eq!(nstype_enum, LinuxNamespaceType::Network);

        let nstype_str = "network";
        let nstype_enum = LinuxNamespaceType::try_from(nstype_str).unwrap();
        assert_eq!(nstype_enum, LinuxNamespaceType::Network);

        let nstype_str = "ipc";
        let nstype_enum = LinuxNamespaceType::try_from(nstype_str).unwrap();
        assert_eq!(nstype_enum, LinuxNamespaceType::Ipc);

        let nstype_str = "cgroup";
        let nstype_enum = LinuxNamespaceType::try_from(nstype_str).unwrap();
        assert_eq!(nstype_enum, LinuxNamespaceType::Cgroup);

        let nstype_str = "mount";
        let nstype_enum = LinuxNamespaceType::try_from(nstype_str).unwrap();
        assert_eq!(nstype_enum, LinuxNamespaceType::Mount);

        let invalid_nstype_str = "xxx";
        let unknown_nstype = LinuxNamespaceType::try_from(invalid_nstype_str);
        assert!(unknown_nstype.is_err());
    }

    // LinuxSeccompAction test cases
    #[test]
    fn seccomp_action_enum_to_string() {
        let type_a = LinuxSeccompAction::ScmpActKill;
        assert_eq!(type_a.to_string(), "SCMP_ACT_KILL");

        let type_b = LinuxSeccompAction::ScmpActAllow;
        assert_eq!(type_b.to_string(), "SCMP_ACT_ALLOW");

        let type_c = LinuxSeccompAction::ScmpActNotify;
        assert_eq!(type_c.to_string(), "SCMP_ACT_NOTIFY");
    }

    #[test]
    fn seccomp_action_string_to_enum() {
        let action_str = "SCMP_ACT_KILL";
        let action_enum: LinuxSeccompAction = action_str.parse().unwrap();
        assert_eq!(action_enum, LinuxSeccompAction::ScmpActKill);

        let action_str = "SCMP_ACT_ALLOW";
        let action_enum: LinuxSeccompAction = action_str.parse().unwrap();
        assert_eq!(action_enum, LinuxSeccompAction::ScmpActAllow);

        let action_str = "SCMP_ACT_NOTIFY";
        let action_enum: LinuxSeccompAction = action_str.parse().unwrap();
        assert_eq!(action_enum, LinuxSeccompAction::ScmpActNotify);

        let invalid_action_str = "x";
        let unknown_action = invalid_action_str.parse::<LinuxSeccompAction>();
        assert!(unknown_action.is_err());
    }

    // LinuxSeccomp Arch test cases
    #[test]
    fn seccomp_arch_enum_to_string() {
        let type_a = Arch::ScmpArchX86_64;
        assert_eq!(type_a.to_string(), "SCMP_ARCH_X86_64");

        let type_b = Arch::ScmpArchAarch64;
        assert_eq!(type_b.to_string(), "SCMP_ARCH_AARCH64");

        let type_c = Arch::ScmpArchPpc64le;
        assert_eq!(type_c.to_string(), "SCMP_ARCH_PPC64LE");
    }

    #[test]
    fn seccomp_arch_string_to_enum() {
        let arch_type_str = "SCMP_ARCH_X86_64";
        let arch_type_enum: Arch = arch_type_str.parse().unwrap();
        assert_eq!(arch_type_enum, Arch::ScmpArchX86_64);

        let arch_type_str = "SCMP_ARCH_AARCH64";
        let arch_type_enum: Arch = arch_type_str.parse().unwrap();
        assert_eq!(arch_type_enum, Arch::ScmpArchAarch64);

        let arch_type_str = "SCMP_ARCH_PPC64LE";
        let arch_type_enum: Arch = arch_type_str.parse().unwrap();
        assert_eq!(arch_type_enum, Arch::ScmpArchPpc64le);

        let invalid_arch_str = "x";
        let unknown_arch = invalid_arch_str.parse::<Arch>();
        assert!(unknown_arch.is_err());
    }

    // LinuxSeccompFilterFlag test cases
    #[test]
    fn seccomp_filter_flag_enum_to_string() {
        let type_a = LinuxSeccompFilterFlag::SeccompFilterFlagLog;
        assert_eq!(type_a.to_string(), "SECCOMP_FILTER_FLAG_LOG");

        let type_b = LinuxSeccompFilterFlag::SeccompFilterFlagTsync;
        assert_eq!(type_b.to_string(), "SECCOMP_FILTER_FLAG_TSYNC");

        let type_c = LinuxSeccompFilterFlag::SeccompFilterFlagSpecAllow;
        assert_eq!(type_c.to_string(), "SECCOMP_FILTER_FLAG_SPEC_ALLOW");
    }

    #[test]
    fn seccomp_filter_flag_string_to_enum() {
        let filter_flag_type_str = "SECCOMP_FILTER_FLAG_LOG";
        let filter_flag_type_enum: LinuxSeccompFilterFlag = filter_flag_type_str.parse().unwrap();
        assert_eq!(
            filter_flag_type_enum,
            LinuxSeccompFilterFlag::SeccompFilterFlagLog
        );

        let filter_flag_type_str = "SECCOMP_FILTER_FLAG_TSYNC";
        let filter_flag_type_enum: LinuxSeccompFilterFlag = filter_flag_type_str.parse().unwrap();
        assert_eq!(
            filter_flag_type_enum,
            LinuxSeccompFilterFlag::SeccompFilterFlagTsync
        );

        let filter_flag_type_str = "SECCOMP_FILTER_FLAG_SPEC_ALLOW";
        let filter_flag_type_enum: LinuxSeccompFilterFlag = filter_flag_type_str.parse().unwrap();
        assert_eq!(
            filter_flag_type_enum,
            LinuxSeccompFilterFlag::SeccompFilterFlagSpecAllow
        );

        let invalid_filter_flag_str = "x";
        let unknown_arch = invalid_filter_flag_str.parse::<LinuxSeccompFilterFlag>();
        assert!(unknown_arch.is_err());
    }

    // LinuxSeccompOperator test cases
    #[test]
    fn seccomp_operator_enum_to_string() {
        let type_a = LinuxSeccompOperator::ScmpCmpNe;
        assert_eq!(type_a.to_string(), "SCMP_CMP_NE");

        let type_b = LinuxSeccompOperator::ScmpCmpMaskedEq;
        assert_eq!(type_b.to_string(), "SCMP_CMP_MASKED_EQ");

        let type_c = LinuxSeccompOperator::ScmpCmpGt;
        assert_eq!(type_c.to_string(), "SCMP_CMP_GT");
    }

    #[test]
    fn seccomp_operator_string_to_enum() {
        let seccomp_operator_str = "SCMP_CMP_GT";
        let seccomp_operator_enum: LinuxSeccompOperator = seccomp_operator_str.parse().unwrap();
        assert_eq!(seccomp_operator_enum, LinuxSeccompOperator::ScmpCmpGt);

        let seccomp_operator_str = "SCMP_CMP_NE";
        let seccomp_operator_enum: LinuxSeccompOperator = seccomp_operator_str.parse().unwrap();
        assert_eq!(seccomp_operator_enum, LinuxSeccompOperator::ScmpCmpNe);

        let seccomp_operator_str = "SCMP_CMP_MASKED_EQ";
        let seccomp_operator_enum: LinuxSeccompOperator = seccomp_operator_str.parse().unwrap();
        assert_eq!(seccomp_operator_enum, LinuxSeccompOperator::ScmpCmpMaskedEq);

        let invalid_seccomp_operator_str = "x";
        let unknown_operator = invalid_seccomp_operator_str.parse::<LinuxSeccompOperator>();
        assert!(unknown_operator.is_err());
    }
}
