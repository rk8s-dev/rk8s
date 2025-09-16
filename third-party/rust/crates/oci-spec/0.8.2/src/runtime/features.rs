use std::collections::HashMap;

use crate::{
    error::OciSpecError,
    runtime::{Arch, LinuxNamespaceType, LinuxSeccompAction},
};
use derive_builder::Builder;
use getset::{Getters, MutGetters, Setters};
use serde::{Deserialize, Serialize};

/// Features represents supported features of the runtime.
///
/// This structure is used to report the supported features of the runtime to runtime callers.
///
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct Features {
    /// The minimum OCI Runtime Spec version recognized by the runtime, e.g., "1.0.0".
    oci_version_min: String,
    /// The maximum OCI Runtime Spec version recognized by the runtime, e.g., "1.0.2-dev".
    oci_version_max: String,
    /// The list of the recognized hook names, e.g., "createRuntime".
    /// "None" means "unknown", not "no support for any hook".
    hooks: Option<Vec<String>>,
    /// The list of the recognized mount options, e.g., "ro".
    /// "None" means "unknown", not "no support for any mount option".
    /// This list does not contain filesystem-specific options passed to mount(2) syscall as (const void *).
    mount_options: Option<Vec<String>>,
    /// Information specific to Linux
    linux: Option<LinuxFeature>,
    /// Implementation-specific annotation strings,
    /// such as the implementation version, and third-party extensions.
    annotations: Option<HashMap<String, String>>,
    /// The list of the potential unsafe annotations
    /// that may appear in `config.json`.
    /// A value that ends with "." is interpreted as a prefix of annotations.
    potentially_unsafe_config_annotations: Option<Vec<String>>,
}

/// Linux specific features.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct LinuxFeature {
    /// The list of the recognized namespaces, e.g., "mount".
    /// "None" means "unknown", not "no support for any namespace".
    namespaces: Option<Vec<LinuxNamespaceType>>,
    /// The list of the recognized capabilities , e.g., "CAP_SYS_ADMIN".
    /// "None" means "unknown", not "no support for any capability".
    capabilities: Option<Vec<String>>,
    /// The available features related to cgroup.
    cgroup: Option<Cgroup>,
    /// The available features related to seccomp.
    seccomp: Option<Seccomp>,
    /// The available features related to apparmor.
    apparmor: Option<Apparmor>,
    /// The available features related to selinux.
    selinux: Option<Selinux>,
    /// The available features related to Intel RDT.
    intel_rdt: Option<IntelRdt>,
    /// The available features related to mount extensions.
    mount_extensions: Option<MountExtensions>,
    /// The available features related to net devices.
    net_devices: Option<NetDevices>,
}

/// Cgroup represents the "cgroup" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct Cgroup {
    /// "v1" field represents whether Cgroup v1 support is compiled in.
    /// Unrelated to whether the host uses cgroup v1 or not.
    /// "None" means "unknown", not "false".
    v1: Option<bool>,
    /// "v2" field represents whether Cgroup v2 support is compiled in.
    /// Unrelated to whether the host uses cgroup v2 or not.
    /// "None" means "unknown", not "false".
    v2: Option<bool>,
    /// "systemd" field represents whether systemd-cgroup support is compiled in.
    /// Unrelated to whether the host uses systemd or not.
    /// "None" means "unknown", not "false".
    systemd: Option<bool>,
    /// "systemdUser" field represents whether user-scoped systemd-cgroup support is compiled in.
    /// Unrelated to whether the host uses systemd or not.
    /// "None" means "unknown", not "false".
    systemd_user: Option<bool>,
    /// "rdma" field represents whether RDMA cgroup support is compiled in.
    /// Unrelated to whether the host supports it or not.
    /// "None" means "unknown", not "false".
    rdma: Option<bool>,
}

/// Seccomp represents the "seccomp" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct Seccomp {
    /// "enabled" field represents whether seccomp support is compiled in.
    /// "None" means "unknown", not "false".
    enabled: Option<bool>,
    /// "actions" field represents the list of the recognized actions.
    /// "None" means "unknown", not "no support for any action".
    actions: Option<Vec<LinuxSeccompAction>>,
    /// "operators" field represents the list of the recognized operators.
    /// "None" means "unknown", not "no support for any operator".
    operators: Option<Vec<String>>,
    /// "archs" field represents the list of the recognized architectures.
    /// "None" means "unknown", not "no support for any architecture".
    archs: Option<Vec<Arch>>,
    /// "knownFlags" field represents the list of the recognized filter flags.
    /// "None" means "unknown", not "no flags are recognized".
    known_flags: Option<Vec<String>>,
    /// "supportedFlags" field represents the list of the supported filter flags.
    /// This list may be a subset of the "knownFlags" due to some of unsupported flags
    /// by the current kernel and/or libseccomp.
    /// "None" means "unknown", not "no flags are supported".
    supported_flags: Option<Vec<String>>,
}

/// Apparmor represents the "apparmor" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct Apparmor {
    /// "enabled" field represents whether AppArmor support is compiled in.
    /// Unrelated to whether the host supports AppArmor or not.
    /// "None" means "unknown", not "false".
    enabled: Option<bool>,
}

/// Selinux represents the "selinux" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct Selinux {
    /// "enabled" field represents whether SELinux support is compiled in.
    /// Unrelated to whether the host supports SELinux or not.
    /// "None" means "unknown", not "false".
    enabled: Option<bool>,
}

/// IntelRdt represents the "intelRdt" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct IntelRdt {
    /// "enabled" field represents whether Intel RDT support is compiled in.
    /// Unrelated to whether the host supports Intel RDT or not.
    enabled: Option<bool>,
}

/// MountExtensions represents the "mountExtensions" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct MountExtensions {
    /// "idMap" field represents the ID mapping support.
    idmap: Option<IDMap>,
}

/// NetDevices represents the "netDevices" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct NetDevices {
    /// "enabled" field represents whether Net Devices support is compiled in.
    /// Unrelated to whether the host supports Net Devices or not.
    enabled: Option<bool>,
}

/// IDMap represents the "idmap" field.
#[derive(
    Builder,
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    MutGetters,
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
#[getset(get_mut = "pub", get = "pub", set = "pub")]
pub struct IDMap {
    /// "enabled" field represents whether idmap mounts supports is compiled in.
    /// Unrelated to whether the host supports it or not.
    /// "None" means "unknown", not "false".
    enabled: Option<bool>,
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::*;

    #[test]
    fn test_parse_features() {
        let example_json = r#"
{
    "ociVersionMin": "1.0.0",
    "ociVersionMax": "1.1.0-rc.2",
    "hooks": [
        "prestart",
        "createRuntime",
        "createContainer",
        "startContainer",
        "poststart",
        "poststop"
    ],
    "mountOptions": [
        "async",
        "atime",
        "bind",
        "defaults",
        "dev",
        "diratime",
        "dirsync",
        "exec",
        "iversion",
        "lazytime",
        "loud",
        "mand",
        "noatime",
        "nodev",
        "nodiratime",
        "noexec",
        "noiversion",
        "nolazytime",
        "nomand",
        "norelatime",
        "nostrictatime",
        "nosuid",
        "nosymfollow",
        "private",
        "ratime",
        "rbind",
        "rdev",
        "rdiratime",
        "relatime",
        "remount",
        "rexec",
        "rnoatime",
        "rnodev",
        "rnodiratime",
        "rnoexec",
        "rnorelatime",
        "rnostrictatime",
        "rnosuid",
        "rnosymfollow",
        "ro",
        "rprivate",
        "rrelatime",
        "rro",
        "rrw",
        "rshared",
        "rslave",
        "rstrictatime",
        "rsuid",
        "rsymfollow",
        "runbindable",
        "rw",
        "shared",
        "silent",
        "slave",
        "strictatime",
        "suid",
        "symfollow",
        "sync",
        "tmpcopyup",
        "unbindable"
    ],
    "linux": {
        "namespaces": [
            "cgroup",
            "ipc",
            "mount",
            "network",
            "pid",
            "user",
            "uts"
        ],
        "capabilities": [
            "CAP_CHOWN",
            "CAP_DAC_OVERRIDE",
            "CAP_DAC_READ_SEARCH",
            "CAP_FOWNER",
            "CAP_FSETID",
            "CAP_KILL",
            "CAP_SETGID",
            "CAP_SETUID",
            "CAP_SETPCAP",
            "CAP_LINUX_IMMUTABLE",
            "CAP_NET_BIND_SERVICE",
            "CAP_NET_BROADCAST",
            "CAP_NET_ADMIN",
            "CAP_NET_RAW",
            "CAP_IPC_LOCK",
            "CAP_IPC_OWNER",
            "CAP_SYS_MODULE",
            "CAP_SYS_RAWIO",
            "CAP_SYS_CHROOT",
            "CAP_SYS_PTRACE",
            "CAP_SYS_PACCT",
            "CAP_SYS_ADMIN",
            "CAP_SYS_BOOT",
            "CAP_SYS_NICE",
            "CAP_SYS_RESOURCE",
            "CAP_SYS_TIME",
            "CAP_SYS_TTY_CONFIG",
            "CAP_MKNOD",
            "CAP_LEASE",
            "CAP_AUDIT_WRITE",
            "CAP_AUDIT_CONTROL",
            "CAP_SETFCAP",
            "CAP_MAC_OVERRIDE",
            "CAP_MAC_ADMIN",
            "CAP_SYSLOG",
            "CAP_WAKE_ALARM",
            "CAP_BLOCK_SUSPEND",
            "CAP_AUDIT_READ",
            "CAP_PERFMON",
            "CAP_BPF",
            "CAP_CHECKPOINT_RESTORE"
        ],
        "cgroup": {
            "v1": true,
            "v2": true,
            "systemd": true,
            "systemdUser": true,
            "rdma": true
        },
        "seccomp": {
            "enabled": true,
            "actions": [
                "SCMP_ACT_ALLOW",
                "SCMP_ACT_ERRNO",
                "SCMP_ACT_KILL",
                "SCMP_ACT_KILL_PROCESS",
                "SCMP_ACT_KILL_THREAD",
                "SCMP_ACT_LOG",
                "SCMP_ACT_NOTIFY",
                "SCMP_ACT_TRACE",
                "SCMP_ACT_TRAP"
            ],
            "operators": [
                "SCMP_CMP_EQ",
                "SCMP_CMP_GE",
                "SCMP_CMP_GT",
                "SCMP_CMP_LE",
                "SCMP_CMP_LT",
                "SCMP_CMP_MASKED_EQ",
                "SCMP_CMP_NE"
            ],
            "archs": [
                "SCMP_ARCH_AARCH64",
                "SCMP_ARCH_ARM",
                "SCMP_ARCH_MIPS",
                "SCMP_ARCH_MIPS64",
                "SCMP_ARCH_MIPS64N32",
                "SCMP_ARCH_MIPSEL",
                "SCMP_ARCH_MIPSEL64",
                "SCMP_ARCH_MIPSEL64N32",
                "SCMP_ARCH_PPC",
                "SCMP_ARCH_PPC64",
                "SCMP_ARCH_PPC64LE",
                "SCMP_ARCH_RISCV64",
                "SCMP_ARCH_S390",
                "SCMP_ARCH_S390X",
                "SCMP_ARCH_X32",
                "SCMP_ARCH_X86",
                "SCMP_ARCH_X86_64"
            ],
            "knownFlags": [
                "SECCOMP_FILTER_FLAG_TSYNC",
                "SECCOMP_FILTER_FLAG_SPEC_ALLOW",
                "SECCOMP_FILTER_FLAG_LOG"
            ],
            "supportedFlags": [
                "SECCOMP_FILTER_FLAG_TSYNC",
                "SECCOMP_FILTER_FLAG_SPEC_ALLOW",
                "SECCOMP_FILTER_FLAG_LOG"
            ]
        },
        "apparmor": {
            "enabled": true
        },
        "selinux": {
            "enabled": true
        },
        "intelRdt": {
            "enabled": true
        },
        "netDevices": {
            "enabled": true
        }
    },
    "annotations": {
        "io.github.seccomp.libseccomp.version": "2.5.4",
        "org.opencontainers.runc.checkpoint.enabled": "true",
        "org.opencontainers.runc.commit": "v1.1.0-534-g26851168",
        "org.opencontainers.runc.version": "1.1.0+dev"
    }
}"#;

        // Parse and check each field
        let features: Features = serde_json::from_str(example_json).unwrap();
        assert_eq!(features.oci_version_min().deref(), "1.0.0".to_string());
        assert_eq!(features.oci_version_max().deref(), "1.1.0-rc.2".to_string());

        assert_eq!(
            features.hooks.as_ref().unwrap(),
            &[
                "prestart",
                "createRuntime",
                "createContainer",
                "startContainer",
                "poststart",
                "poststop"
            ]
        );

        assert_eq!(
            features.mount_options.as_ref().unwrap(),
            &[
                "async",
                "atime",
                "bind",
                "defaults",
                "dev",
                "diratime",
                "dirsync",
                "exec",
                "iversion",
                "lazytime",
                "loud",
                "mand",
                "noatime",
                "nodev",
                "nodiratime",
                "noexec",
                "noiversion",
                "nolazytime",
                "nomand",
                "norelatime",
                "nostrictatime",
                "nosuid",
                "nosymfollow",
                "private",
                "ratime",
                "rbind",
                "rdev",
                "rdiratime",
                "relatime",
                "remount",
                "rexec",
                "rnoatime",
                "rnodev",
                "rnodiratime",
                "rnoexec",
                "rnorelatime",
                "rnostrictatime",
                "rnosuid",
                "rnosymfollow",
                "ro",
                "rprivate",
                "rrelatime",
                "rro",
                "rrw",
                "rshared",
                "rslave",
                "rstrictatime",
                "rsuid",
                "rsymfollow",
                "runbindable",
                "rw",
                "shared",
                "silent",
                "slave",
                "strictatime",
                "suid",
                "symfollow",
                "sync",
                "tmpcopyup",
                "unbindable"
            ]
        );

        let linux = features.linux().as_ref().unwrap();

        assert_eq!(
            linux.namespaces.as_ref().unwrap(),
            &[
                LinuxNamespaceType::Cgroup,
                LinuxNamespaceType::Ipc,
                LinuxNamespaceType::Mount,
                LinuxNamespaceType::Network,
                LinuxNamespaceType::Pid,
                LinuxNamespaceType::User,
                LinuxNamespaceType::Uts,
            ]
        );

        assert_eq!(
            linux.capabilities.as_ref().unwrap(),
            &[
                "CAP_CHOWN",
                "CAP_DAC_OVERRIDE",
                "CAP_DAC_READ_SEARCH",
                "CAP_FOWNER",
                "CAP_FSETID",
                "CAP_KILL",
                "CAP_SETGID",
                "CAP_SETUID",
                "CAP_SETPCAP",
                "CAP_LINUX_IMMUTABLE",
                "CAP_NET_BIND_SERVICE",
                "CAP_NET_BROADCAST",
                "CAP_NET_ADMIN",
                "CAP_NET_RAW",
                "CAP_IPC_LOCK",
                "CAP_IPC_OWNER",
                "CAP_SYS_MODULE",
                "CAP_SYS_RAWIO",
                "CAP_SYS_CHROOT",
                "CAP_SYS_PTRACE",
                "CAP_SYS_PACCT",
                "CAP_SYS_ADMIN",
                "CAP_SYS_BOOT",
                "CAP_SYS_NICE",
                "CAP_SYS_RESOURCE",
                "CAP_SYS_TIME",
                "CAP_SYS_TTY_CONFIG",
                "CAP_MKNOD",
                "CAP_LEASE",
                "CAP_AUDIT_WRITE",
                "CAP_AUDIT_CONTROL",
                "CAP_SETFCAP",
                "CAP_MAC_OVERRIDE",
                "CAP_MAC_ADMIN",
                "CAP_SYSLOG",
                "CAP_WAKE_ALARM",
                "CAP_BLOCK_SUSPEND",
                "CAP_AUDIT_READ",
                "CAP_PERFMON",
                "CAP_BPF",
                "CAP_CHECKPOINT_RESTORE"
            ],
        );

        assert_eq!(
            linux.cgroup.as_ref().unwrap(),
            &Cgroup {
                v1: Some(true),
                v2: Some(true),
                systemd: Some(true),
                systemd_user: Some(true),
                rdma: Some(true),
            }
        );

        assert_eq!(
            linux.seccomp.as_ref().unwrap(),
            &Seccomp {
                enabled: Some(true),
                actions: Some(vec![
                    LinuxSeccompAction::ScmpActAllow,
                    LinuxSeccompAction::ScmpActErrno,
                    LinuxSeccompAction::ScmpActKill,
                    LinuxSeccompAction::ScmpActKillProcess,
                    LinuxSeccompAction::ScmpActKillThread,
                    LinuxSeccompAction::ScmpActLog,
                    LinuxSeccompAction::ScmpActNotify,
                    LinuxSeccompAction::ScmpActTrace,
                    LinuxSeccompAction::ScmpActTrap
                ]),
                operators: Some(vec![
                    "SCMP_CMP_EQ".to_string(),
                    "SCMP_CMP_GE".to_string(),
                    "SCMP_CMP_GT".to_string(),
                    "SCMP_CMP_LE".to_string(),
                    "SCMP_CMP_LT".to_string(),
                    "SCMP_CMP_MASKED_EQ".to_string(),
                    "SCMP_CMP_NE".to_string()
                ]),
                archs: Some(vec![
                    Arch::ScmpArchAarch64,
                    Arch::ScmpArchArm,
                    Arch::ScmpArchMips,
                    Arch::ScmpArchMips64,
                    Arch::ScmpArchMips64n32,
                    Arch::ScmpArchMipsel,
                    Arch::ScmpArchMipsel64,
                    Arch::ScmpArchMipsel64n32,
                    Arch::ScmpArchPpc,
                    Arch::ScmpArchPpc64,
                    Arch::ScmpArchPpc64le,
                    Arch::ScmpArchRiscv64,
                    Arch::ScmpArchS390,
                    Arch::ScmpArchS390x,
                    Arch::ScmpArchX32,
                    Arch::ScmpArchX86,
                    Arch::ScmpArchX86_64,
                ]),
                known_flags: Some(vec![
                    "SECCOMP_FILTER_FLAG_TSYNC".to_string(),
                    "SECCOMP_FILTER_FLAG_SPEC_ALLOW".to_string(),
                    "SECCOMP_FILTER_FLAG_LOG".to_string()
                ]),
                supported_flags: Some(vec![
                    "SECCOMP_FILTER_FLAG_TSYNC".to_string(),
                    "SECCOMP_FILTER_FLAG_SPEC_ALLOW".to_string(),
                    "SECCOMP_FILTER_FLAG_LOG".to_string()
                ])
            },
        );

        assert_eq!(
            linux.apparmor.as_ref().unwrap(),
            &Apparmor {
                enabled: Some(true)
            }
        );

        assert_eq!(
            linux.selinux.as_ref().unwrap(),
            &Selinux {
                enabled: Some(true)
            }
        );

        assert_eq!(
            linux.intel_rdt.as_ref().unwrap(),
            &IntelRdt {
                enabled: Some(true)
            }
        );

        assert_eq!(
            linux.net_devices.as_ref().unwrap(),
            &NetDevices {
                enabled: Some(true)
            }
        );

        assert_eq!(
            features.annotations().as_ref().unwrap(),
            &[
                (
                    "io.github.seccomp.libseccomp.version".to_string(),
                    "2.5.4".to_string()
                ),
                (
                    "org.opencontainers.runc.checkpoint.enabled".to_string(),
                    "true".to_string()
                ),
                (
                    "org.opencontainers.runc.commit".to_string(),
                    "v1.1.0-534-g26851168".to_string()
                ),
                (
                    "org.opencontainers.runc.version".to_string(),
                    "1.1.0+dev".to_string()
                )
            ]
            .iter()
            .cloned()
            .collect()
        );

        assert_eq!(
            features.potentially_unsafe_config_annotations().as_ref(),
            None,
        );
    }
}
