use lazy_static::lazy_static;
use libcontainer::oci_spec::runtime::{
    Capability, LinuxBuilder, LinuxCapabilities, LinuxNamespaceBuilder, LinuxNamespaceType,
    MountBuilder, ProcessBuilder, Spec,
};
use tracing::debug;

use crate::cri::cri_api::ContainerConfig;
use anyhow::{Result, anyhow};
use common::ContainerSpec;
use oci_spec::runtime::{HookBuilder, HooksBuilder, LinuxNamespace, RootBuilder};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

// Default supported capabilities (from docker's implementation)
lazy_static! {
    pub static ref DEFAULT_CAPABILITIES: Vec<Capability> = {
        vec![
            Capability::Chown,
            Capability::DacOverride,
            Capability::Fsetid,
            Capability::Fowner,
            Capability::Mknod,
            Capability::NetRaw,
            Capability::Setgid,
            Capability::Setuid,
            Capability::Setfcap,
            Capability::Setpcap,
            Capability::NetBindService,
            Capability::SysChroot,
            Capability::Kill,
            Capability::AuditWrite,
        ]
    };
}

pub struct OCISpecGenerator {
    inner_spec: Spec,
    container_config: ContainerConfig,
    container_spec: ContainerSpec,
    pause_pid: Option<i32>,
}

impl OCISpecGenerator {
    pub fn new(config: &ContainerConfig, spec: &ContainerSpec, pause_pid: Option<i32>) -> Self {
        Self {
            inner_spec: Spec::default(),
            container_config: config.clone(),
            container_spec: spec.clone(),
            pause_pid,
        }
    }

    fn get_capabilities(&self) -> Result<LinuxCapabilities> {
        let mut capabilities = new_linux_capabilities_with_defaults();

        // Handle the passed capabilities from users
        if let Some(ctx) = &self.container_spec.security_context {
            // FIXME: Handle the privileged Mode
            // Note: Privileged mode typically requires additional configurations, such as:
            // - Disable Seccomp filters.
            // - Disable AppArmor/SELinux profiles.
            // - Allow access to all /dev devices.
            // give all the available capabilities
            if let Some(p) = ctx.privileged
                && p
            {
                set_all_capabilities(&mut capabilities);
            }

            if let Some(caps) = &ctx.capabilities {
                caps.add.iter().for_each(|cap| {
                    add_cap(*cap, &mut capabilities);
                });

                caps.drop.iter().for_each(|cap| {
                    drop_cap(cap, &mut capabilities);
                });
            }
        }

        Ok(capabilities)
    }

    pub fn process_set(&mut self) -> Result<()> {
        let mut process = ProcessBuilder::default().build()?;

        // Choose from container_spec(first) or container_config(from container's image)
        let arg = if self.container_spec.args.is_empty() {
            self.container_config.args.clone()
        } else {
            self.container_spec.args.clone()
        };
        process.set_args(Some(arg));
        process.set_terminal(Some(self.container_spec.tty));
        process.set_env(Some(self.build_process_env(&process)));

        let capabilities = self.get_capabilities()?;
        process.set_capabilities(Some(capabilities));

        self.inner_spec.set_process(Some(process));
        Ok(())
    }

    pub fn generate(mut self) -> Result<Spec> {
        let root = RootBuilder::default().readonly(false).build()?;
        self.inner_spec.set_root(Some(root));

        let namespaces = self
            .create_container_namespaces()
            .map_err(|e| anyhow!("failed to setup Linux namespace: {e}"))?;

        self.process_set()
            .map_err(|e| anyhow!("failed to setup oci process: {e}"))?;

        let mut linux_builder = LinuxBuilder::default().namespaces(namespaces);

        if let Some(linux_config) = &self.container_config.linux
            && let Some(resources) = &linux_config.resources
        {
            linux_builder = linux_builder.resources(&resources.clone());
        }

        let linux = linux_builder.build()?;
        self.inner_spec.set_linux(Some(linux));

        // Convert CRI mounts to OCI mounts
        if !self.container_config.mounts.is_empty() {
            let mut oci_mounts = self.inner_spec.mounts().clone().unwrap_or_default();
            for m in &self.container_config.mounts {
                let mut opts = vec![
                    if m.readonly { "ro" } else { "rw" }.to_string(),
                    "rbind".to_string(),
                ];
                if m.propagation != 0 {
                    opts.push("rprivate".to_string());
                }
                let mount_type = if m.host_path.starts_with('/') {
                    "bind"
                } else {
                    "none"
                };
                let oci_mount = MountBuilder::default()
                    .typ(mount_type)
                    .destination(&m.container_path)
                    .source(&m.host_path)
                    .options(opts)
                    .build()?;
                oci_mounts.push(oci_mount);
            }
            self.inner_spec.set_mounts(Some(oci_mounts));
        }

        if let Some(hooks) = self.build_gpu_hooks()? {
            self.inner_spec.set_hooks(Some(hooks));
            debug!("GPU hooks added to OCI spec: {:?}", self.inner_spec.hooks());
        }

        Ok(self.inner_spec)
    }

    fn build_process_env(&self, process: &oci_spec::runtime::Process) -> Vec<String> {
        let mut env_map = BTreeMap::new();

        for entry in process.env().as_ref().cloned().unwrap_or_default() {
            if let Some((key, value)) = split_env_entry(&entry) {
                env_map.insert(key.to_string(), value.to_string());
            }
        }

        for entry in self.gpu_env_vars() {
            if let Some((key, value)) = split_env_entry(&entry) {
                env_map.insert(key.to_string(), value.to_string());
            }
        }

        env_map
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect()
    }

    fn build_gpu_hooks(&self) -> Result<Option<oci_spec::runtime::Hooks>> {
        let Some(gpu_spec) = &self.container_spec.gpus else {
            return Ok(None);
        };
        debug!("In build_gpu_hooks() GPU spec found: {:?}", gpu_spec);
        if !gpu_spec.enabled {
            return Ok(None);
        }

        let hook_path = find_nvidia_container_runtime_hook().ok_or_else(|| {
            anyhow!(
                "GPU support requested for container '{}' but nvidia-container-runtime-hook was not found in PATH or standard locations",
                self.container_spec.name
            )
        })?;

        let hook = HookBuilder::default()
            .path(hook_path)
            .args(vec![
                "nvidia-container-runtime-hook".to_string(),
                "prestart".to_string(),
            ])
            .build()?;

        let hooks = HooksBuilder::default().create_runtime(vec![hook]).build()?;
        Ok(Some(hooks))
    }

    fn gpu_env_vars(&self) -> Vec<String> {
        let Some(gpu_spec) = &self.container_spec.gpus else {
            return vec![];
        };
        if !gpu_spec.enabled {
            return vec![];
        }

        let visible_devices = if gpu_spec.device_ids.is_empty() {
            "all".to_string()
        } else {
            gpu_spec.device_ids.join(",")
        };

        let driver_capabilities = if gpu_spec.driver_capabilities.is_empty() {
            "utility,compute".to_string()
        } else {
            gpu_spec.driver_capabilities.join(",")
        };

        vec![
            format!("NVIDIA_VISIBLE_DEVICES={visible_devices}"),
            format!("NVIDIA_DRIVER_CAPABILITIES={driver_capabilities}"),
        ]
    }

    fn create_container_namespaces(&self) -> Result<Vec<LinuxNamespace>> {
        let mut namespaces = Vec::new();

        // Mount and Cgroup will always be specific
        namespaces.push(
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Mount)
                .build()?,
        );
        namespaces.push(
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Cgroup)
                .build()?,
        );

        if let Some(pid) = self.pause_pid {
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Pid)
                    .path(format!("/proc/{pid}/ns/pid"))
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Network)
                    .path(format!("/proc/{pid}/ns/net"))
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Ipc)
                    .path(format!("/proc/{pid}/ns/ipc"))
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Uts)
                    .path(format!("/proc/{pid}/ns/uts"))
                    .build()?,
            );
        } else {
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Pid)
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Network)
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Ipc)
                    .build()?,
            );
            namespaces.push(
                LinuxNamespaceBuilder::default()
                    .typ(LinuxNamespaceType::Uts)
                    .build()?,
            );
        }

        Ok(namespaces)
    }
}

fn find_nvidia_container_runtime_hook() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "/usr/bin/nvidia-container-runtime-hook",
        "/usr/local/bin/nvidia-container-runtime-hook",
        "/bin/nvidia-container-runtime-hook",
    ];

    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            let candidate = dir.join("nvidia-container-runtime-hook");
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn split_env_entry(entry: &str) -> Option<(&str, &str)> {
    entry.split_once('=')
}

macro_rules! drop_capability_from_set {
    ($caps:expr, $getter:ident, $setter:ident, $cap:expr) => {
        let mut set = $caps.$getter().as_ref().cloned().unwrap_or_default();

        set.remove($cap);

        $caps.$setter(Some(set));
    };
}

macro_rules! add_capability_to_set {
    ($caps:expr, $getter:ident, $setter:ident, $cap:expr) => {
        let mut set = $caps.$getter().as_ref().cloned().unwrap_or_default();

        set.insert($cap.clone());

        $caps.$setter(Some(set));
    };
}

fn get_default_set() -> Option<HashSet<Capability>> {
    Some(DEFAULT_CAPABILITIES.iter().cloned().collect())
}

pub fn new_linux_capabilities_with_defaults() -> LinuxCapabilities {
    let mut capabilities = LinuxCapabilities::default();

    let default_set = get_default_set();
    capabilities.set_bounding(default_set.clone());
    capabilities.set_effective(default_set.clone());
    capabilities.set_inheritable(default_set.clone());
    capabilities.set_permitted(default_set.clone());
    capabilities.set_ambient(default_set);

    capabilities
}

pub fn drop_cap(cap: &Capability, capabilities: &mut LinuxCapabilities) {
    drop_capability_from_set!(capabilities, bounding, set_bounding, cap);
    drop_capability_from_set!(capabilities, effective, set_effective, cap);
    drop_capability_from_set!(capabilities, inheritable, set_inheritable, cap);
    drop_capability_from_set!(capabilities, permitted, set_permitted, cap);
    drop_capability_from_set!(capabilities, ambient, set_ambient, cap);
}

pub fn add_cap(cap: Capability, capabilities: &mut LinuxCapabilities) {
    add_capability_to_set!(capabilities, bounding, set_bounding, cap);
    add_capability_to_set!(capabilities, effective, set_effective, cap);
    add_capability_to_set!(capabilities, inheritable, set_inheritable, cap);
    add_capability_to_set!(capabilities, permitted, set_permitted, cap);
    add_capability_to_set!(capabilities, ambient, set_ambient, cap);
}

pub fn set_all_capabilities(capabilities: &mut LinuxCapabilities) {
    let all_caps = get_all_capabilities();
    capabilities.set_bounding(Some(all_caps.clone()));
    capabilities.set_effective(Some(all_caps.clone()));
    capabilities.set_inheritable(Some(all_caps.clone()));
    capabilities.set_permitted(Some(all_caps.clone()));
    capabilities.set_ambient(Some(all_caps));
}
fn get_all_capabilities() -> HashSet<Capability> {
    [
        Capability::Chown,
        Capability::DacOverride,
        Capability::DacReadSearch,
        Capability::Fowner,
        Capability::Fsetid,
        Capability::Kill,
        Capability::Setgid,
        Capability::Setuid,
        Capability::Setpcap,
        Capability::LinuxImmutable,
        Capability::NetBindService,
        Capability::NetBroadcast,
        Capability::NetAdmin,
        Capability::NetRaw,
        Capability::IpcLock,
        Capability::IpcOwner,
        Capability::SysModule,
        Capability::SysRawio,
        Capability::SysChroot,
        Capability::SysPtrace,
        Capability::SysPacct,
        Capability::SysAdmin,
        Capability::SysBoot,
        Capability::SysNice,
        Capability::SysResource,
        Capability::SysTime,
        Capability::SysTtyConfig,
        Capability::Mknod,
        Capability::Lease,
        Capability::AuditWrite,
        Capability::AuditControl,
        Capability::Setfcap,
        Capability::MacOverride,
        Capability::MacAdmin,
        Capability::Syslog,
        Capability::WakeAlarm,
        Capability::BlockSuspend,
        Capability::AuditRead,
        Capability::Perfmon,
        Capability::Bpf,
        Capability::CheckpointRestore,
    ]
    .into_iter()
    .collect::<HashSet<_>>()
}
