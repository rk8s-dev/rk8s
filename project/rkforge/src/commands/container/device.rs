use super::ContainerRunner;
use anyhow::{Context, Result, anyhow};
use common::ContainerSpec;
use libruntime::{cri::cri_api::Device, oci};
use oci_spec::runtime::{
    Linux, LinuxCapabilities, LinuxDevice, LinuxDeviceBuilder, LinuxDeviceCgroup,
    LinuxDeviceCgroupBuilder, LinuxDeviceType,
};
use std::{
    collections::HashSet,
    env, fs, io,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
};
use tracing::warn;

const DEFAULT_DEVICE_PERMISSIONS: &str = "rwm";
const DEFAULT_CONTAINER_DEVICE_MODE: u32 = 0o660;
pub(super) const RUN_DEVICES_ENV: &str = "RKFORGE_RUN_DEVICES";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceRequest {
    host_path: PathBuf,
    container_path: PathBuf,
    permissions: String,
}

impl DeviceRequest {
    fn parse(raw: &str) -> Result<Self> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(anyhow!("--device value cannot be empty"));
        }

        let parts: Vec<&str> = value.splitn(3, ':').collect();
        let host_path = parts[0].trim();
        if host_path.is_empty() {
            return Err(anyhow!("device host path cannot be empty in '{value}'"));
        }

        let container_path = match parts.get(1).map(|part| part.trim()) {
            None | Some("") => host_path,
            Some(path) => path,
        };

        let permissions = match parts.get(2).map(|part| part.trim()) {
            None | Some("") => DEFAULT_DEVICE_PERMISSIONS.to_string(),
            Some(perms) => normalize_device_permissions(perms)?,
        };

        let host_path = PathBuf::from(host_path);
        if !host_path.is_absolute() {
            return Err(anyhow!(
                "Device {} must be an absolute path on host",
                host_path.display()
            ));
        }

        let container_path = PathBuf::from(container_path);
        if !container_path.is_absolute() {
            return Err(anyhow!(
                "Container device path {} must be an absolute path",
                container_path.display()
            ));
        }

        let _ = inspect_host_device(&host_path)?;

        Ok(Self {
            host_path,
            container_path,
            permissions,
        })
    }

    fn to_cri_device(&self) -> Device {
        Device {
            container_path: self.container_path.to_string_lossy().into_owned(),
            host_path: self.host_path.to_string_lossy().into_owned(),
            permissions: self.permissions.clone(),
        }
    }

    fn looks_like_gpu(&self) -> bool {
        self.host_path.starts_with("/dev/dri")
            || self.host_path.starts_with("/dev/kfd")
            || self.host_path.starts_with("/dev/nvidia")
    }
}

// Device related logic for container runner
impl ContainerRunner {
    pub(super) fn set_requested_devices(&mut self, requested_devices: Vec<DeviceRequest>) {
        self.requested_devices = requested_devices;
    }

    pub(super) fn has_requested_devices(&self) -> bool {
        !self.requested_devices.is_empty()
    }

    fn is_privileged(&self) -> bool {
        self.spec
            .security_context
            .as_ref()
            .and_then(|ctx| ctx.privileged)
            .unwrap_or(false)
    }

    pub(super) fn warn_about_unprivileged_gpu_devices(&self) {
        if self.is_privileged()
            || !self
                .requested_devices
                .iter()
                .any(DeviceRequest::looks_like_gpu)
        {
            return;
        }

        warn!(
            "GPU device passthrough requested without privileged mode. Explicit device mappings will be added, but some GPU stacks may also require `securityContext.privileged: true` or additional --device entries."
        );
    }

    // Transform requsted devices info to actual config device struct
    pub(super) fn apply_requested_devices(&mut self) {
        self.config_builder.devices = self
            .requested_devices
            .iter()
            .map(DeviceRequest::to_cri_device)
            .collect();
    }
}

fn normalize_device_permissions(value: &str) -> Result<String> {
    let mut read = false;
    let mut write = false;
    let mut mknod = false;

    for ch in value.chars() {
        match ch {
            'r' => read = true,
            'w' => write = true,
            'm' => mknod = true,
            _ => {
                return Err(anyhow!(
                    "invalid device permissions '{value}'; expected a combination of 'r', 'w', and 'm'"
                ));
            }
        }
    }

    if !read && !write && !mknod {
        return Err(anyhow!(
            "invalid device permissions '{value}'; expected at least one of 'r', 'w', or 'm'"
        ));
    }

    let mut normalized = String::new();
    if read {
        normalized.push('r');
    }
    if write {
        normalized.push('w');
    }
    if mknod {
        normalized.push('m');
    }

    Ok(normalized)
}

pub(super) fn collect_requested_devices(explicit_devices: &[String]) -> Result<Vec<DeviceRequest>> {
    let raw_devices = if explicit_devices.is_empty() {
        collect_requested_devices_from_env()?
    } else {
        explicit_devices.to_vec()
    };

    let mut requests = Vec::with_capacity(raw_devices.len());
    let mut seen_container_paths = HashSet::new();

    for raw_device in raw_devices {
        let request = DeviceRequest::parse(&raw_device)?;
        if !seen_container_paths.insert(request.container_path.clone()) {
            return Err(anyhow!(
                "Device mapping for {} is duplicated",
                request.container_path.display()
            ));
        }
        requests.push(request);
    }

    Ok(requests)
}

fn collect_requested_devices_from_env() -> Result<Vec<String>> {
    match env::var(RUN_DEVICES_ENV) {
        Ok(value) => Ok(value
            .split([',', '\n'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect()),
        Err(env::VarError::NotPresent) => Ok(vec![]),
        Err(err) => Err(anyhow!("failed to read {RUN_DEVICES_ENV}: {err}")),
    }
}

// Handle the host device's Type transform
fn inspect_host_device(path: &Path) -> Result<(LinuxDeviceType, i64, i64)> {
    let metadata = fs::metadata(path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => anyhow!("Device {} not found on host", path.display()),
        io::ErrorKind::PermissionDenied => anyhow!(
            "Device {} is not accessible on host: {}",
            path.display(),
            err
        ),
        _ => anyhow!("Failed to inspect device {}: {}", path.display(), err),
    })?;

    let file_type = metadata.file_type();
    let device_type = if file_type.is_char_device() {
        LinuxDeviceType::C
    } else if file_type.is_block_device() {
        LinuxDeviceType::B
    } else {
        return Err(anyhow!(
            "Device {} is not a character or block device on host",
            path.display()
        ));
    };

    let major = i64::try_from(nix::sys::stat::major(metadata.rdev()))
        .context("device major number does not fit in i64")?;
    let minor = i64::try_from(nix::sys::stat::minor(metadata.rdev()))
        .context("device minor number does not fit in i64")?;

    Ok((device_type, major, minor))
}

pub(super) fn append_oci_devices(linux: &mut Linux, devices: &[Device]) -> Result<()> {
    if devices.is_empty() {
        return Ok(());
    }

    let mut linux_devices = linux.devices().clone().unwrap_or_default();
    let mut resources = linux.resources().clone().unwrap_or_default();
    let mut device_rules = resources.devices().clone().unwrap_or_default();

    for device in devices {
        let (linux_device, device_rule) = resolve_oci_device(device)?;
        linux_devices.push(linux_device);
        device_rules.push(device_rule);
    }

    resources.set_devices(Some(device_rules));
    linux.set_resources(Some(resources));
    linux.set_devices(Some(linux_devices));

    Ok(())
}

fn resolve_oci_device(device: &Device) -> Result<(LinuxDevice, LinuxDeviceCgroup)> {
    let host_path = Path::new(&device.host_path);
    let (device_type, major, minor) = inspect_host_device(host_path)?;

    let linux_device = LinuxDeviceBuilder::default()
        .path(PathBuf::from(&device.container_path))
        .typ(device_type)
        .major(major)
        .minor(minor)
        .file_mode(DEFAULT_CONTAINER_DEVICE_MODE)
        .uid(0u32)
        .gid(0u32)
        .build()?;

    let cgroup_rule = LinuxDeviceCgroupBuilder::default()
        .allow(true)
        .typ(device_type)
        .major(major)
        .minor(minor)
        .access(device.permissions.clone())
        .build()?;

    Ok((linux_device, cgroup_rule))
}

// Handle Capabilities Set Logic(Support security_contex)
pub(super) fn build_process_capabilities(spec: &ContainerSpec) -> LinuxCapabilities {
    let mut capabilities = oci::new_linux_capabilities_with_defaults();

    if let Some(ctx) = &spec.security_context {
        if ctx.privileged.unwrap_or(false) {
            oci::set_all_capabilities(&mut capabilities);
        }

        if let Some(extra_caps) = &ctx.capabilities {
            for capability in &extra_caps.add {
                oci::add_cap(*capability, &mut capabilities);
            }

            for capability in &extra_caps.drop {
                oci::drop_cap(capability, &mut capabilities);
            }
        }
    }

    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::container::ContainerRunner;
    use serial_test::serial;
    use tempfile::tempdir;

    #[test]
    #[serial]
    fn test_collect_requested_devices_prefers_cli_over_env() {
        // SAFETY: serialized test; variable is restored before the test exits.
        unsafe { env::set_var(RUN_DEVICES_ENV, "/dev/zero") };

        let devices = collect_requested_devices(&["/dev/null".to_string()]).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].host_path, PathBuf::from("/dev/null"));

        // SAFETY: serialized test; cleanup matches the set above.
        unsafe { env::remove_var(RUN_DEVICES_ENV) };
    }

    #[test]
    #[serial]
    fn test_collect_requested_devices_reads_env() {
        // SAFETY: serialized test; variable is restored before the test exits.
        unsafe { env::set_var(RUN_DEVICES_ENV, "/dev/null,/dev/zero::rw") };

        let devices = collect_requested_devices(&[]).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].host_path, PathBuf::from("/dev/null"));
        assert_eq!(devices[1].permissions, "rw");

        // SAFETY: serialized test; cleanup matches the set above.
        unsafe { env::remove_var(RUN_DEVICES_ENV) };
    }

    #[test]
    fn test_collect_requested_devices_rejects_missing_paths() {
        let err = collect_requested_devices(&["/definitely-missing-rkforge-device".to_string()])
            .unwrap_err()
            .to_string();

        assert!(err.contains("not found on host"));
    }

    #[test]
    fn test_create_oci_spec_includes_requested_devices() {
        let bundle_dir = tempdir().unwrap();
        let bundle_path = bundle_dir.path().to_string_lossy().to_string();
        let mut runner = ContainerRunner::from_spec(
            ContainerSpec {
                name: "gpu-demo".to_string(),
                image: bundle_path,
                ports: vec![],
                args: vec!["/bin/echo".to_string(), "hi".to_string()],
                resources: None,
                liveness_probe: None,
                readiness_probe: None,
                startup_probe: None,
                security_context: None,
                env: None,
                volume_mounts: None,
                command: None,
                working_dir: None,
            },
            None,
        )
        .unwrap();

        runner
            .set_requested_devices(collect_requested_devices(&["/dev/null".to_string()]).unwrap());
        runner.build_config().unwrap();

        let spec = runner.create_oci_spec().unwrap();
        let linux = spec.linux().as_ref().unwrap();
        let devices = linux.devices().as_ref().unwrap();
        let device_rules = linux
            .resources()
            .as_ref()
            .unwrap()
            .devices()
            .as_ref()
            .unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path(), &PathBuf::from("/dev/null"));
        assert_eq!(devices[0].file_mode(), Some(DEFAULT_CONTAINER_DEVICE_MODE));
        assert_eq!(devices[0].uid(), Some(0));
        assert_eq!(devices[0].gid(), Some(0));
        assert_eq!(device_rules.len(), 1);
        assert_eq!(device_rules[0].access().as_deref(), Some("rwm"));
    }
}
