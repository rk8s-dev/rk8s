use anyhow::{Ok, Result, anyhow};
use oci_spec::runtime::{
    LinuxCpuBuilder, LinuxMemoryBuilder, LinuxResources, LinuxResourcesBuilder,
};
use std::collections::HashMap;
use thiserror::Error;

use crate::cri::cri_api::{
    CdiDevice, ContainerConfig, ContainerMetadata, Device, ImageSpec, KeyValue,
    LinuxContainerConfig, LinuxContainerResources, Mount, WindowsContainerConfig,
};
use common::{ContainerRes, ContainerSpec};

#[allow(unused)]
#[derive(Error, Debug)]
pub enum ConfigParseError {
    #[error("invalid env vectors from image config")]
    InvalidEnvFromImageConfig,
}

#[derive(Clone)]
pub struct ContainerConfigBuilder {
    pub metadata: Option<ContainerMetadata>,
    pub image: Option<ImageSpec>,
    pub command: Vec<String>,
    pub args: Option<Vec<String>>,
    pub working_dir: Option<String>,
    pub envs: Vec<KeyValue>,
    pub mounts: Vec<Mount>,
    pub devices: Vec<Device>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub log_path: String,
    pub stdin: bool,
    pub stdin_once: bool,
    pub tty: bool,
    pub linux: Option<LinuxContainerConfig>,
    pub windows: Option<WindowsContainerConfig>,
    pub cdi_devices: Vec<CdiDevice>,
    pub stop_signal: i32,
}

impl Default for ContainerConfigBuilder {
    fn default() -> Self {
        Self {
            metadata: None,
            image: None,
            command: vec!["bin/sh".to_string()],
            args: None,
            working_dir: Some(String::from("/")),
            envs: vec![KeyValue {
                key: "PATH".to_string(),
                value: "usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            }],
            mounts: vec![],
            devices: vec![],
            labels: HashMap::new(),
            annotations: HashMap::new(),
            log_path: "".to_string(),
            stdin: false,
            stdin_once: false,
            tty: false,
            linux: None,
            windows: None,
            cdi_devices: vec![],
            stop_signal: 0,
        }
    }
}

impl ContainerConfigBuilder {
    pub fn container_spec(&mut self, spec: ContainerSpec) -> Result<&mut Self> {
        let metadata = Some(ContainerMetadata {
            name: spec.name.clone(),
            attempt: 0,
        });

        let image = Some(ImageSpec {
            image: spec.image.clone(),
            annotations: std::collections::HashMap::new(),
            user_specified_image: spec.image.clone(),
            runtime_handler: String::new(),
        });

        let log_path = format!("{}/0.log", spec.name);
        let linux = get_linux_container_config(spec.resources.clone())?;
        if !spec.args.is_empty() {
            self.args = Some(spec.args.clone());
        }

        self.metadata = metadata;
        self.image = image;
        self.log_path = log_path;
        self.linux = linux;

        Ok(self)
    }

    // entrypoints + cmd
    pub fn args_from_image_config(
        &mut self,
        entrypoints: &Option<Vec<String>>,
        cmd: &Option<Vec<String>>,
    ) -> &mut Self {
        let mut args: Vec<String> = Vec::new();
        if let Some(entry) = entrypoints {
            args.extend(entry.clone());
            if let Some(command) = cmd {
                args.extend(command.clone());
            }
        } else {
            args.extend(cmd.clone().unwrap_or_default());
        }
        self.args = Some(args);
        self
    }

    pub fn work_dir(&mut self, work_dir: &Option<String>) -> &mut Self {
        self.working_dir = work_dir.clone();
        self
    }

    pub fn envs_from_image_config(&mut self, envs: &Option<Vec<String>>) -> &mut Self {
        if let Some(env) = envs.as_deref() {
            let key_vaule_vecs: Vec<KeyValue> = env
                .iter()
                .map(move |e| {
                    //  pattern: KEY=VALUE
                    // vec[0] = KEY
                    // vec[1] = Vaule
                    let vec: Vec<&str> = e.split("=").collect();
                    KeyValue {
                        key: vec[0].to_string(),
                        value: vec[1].to_string(),
                    }
                })
                .collect();
            self.envs.extend(key_vaule_vecs);
        }
        self
    }

    pub fn mounts(&mut self, mounts: Vec<Mount>) -> &mut Self {
        self.mounts.extend(mounts);
        self
    }

    #[allow(unused)]
    pub fn metadata(&mut self, metadata: ContainerMetadata) -> &mut Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn images(&mut self, image: String) -> &mut Self {
        self.image = Some(ImageSpec {
            image,
            annotations: HashMap::new(),
            user_specified_image: String::new(),
            runtime_handler: String::new(),
        });
        self
    }

    // pub fn mounts(&mut self, envs: Vec<KeyValue>) -> &mut Self {
    //     self.envs.extend(mounts);
    //     self
    // }

    pub fn build(self) -> ContainerConfig {
        ContainerConfig {
            metadata: self.metadata,
            image: self.image,
            command: self.command,
            args: self.args.unwrap_or_default(),
            working_dir: self.working_dir.unwrap_or_else(|| "/".to_string()),
            envs: self.envs,
            mounts: self.mounts,
            devices: self.devices,
            labels: self.labels,
            annotations: self.annotations,
            log_path: self.log_path,
            stdin: self.stdin,
            stdin_once: self.stdin_once,
            tty: self.tty,
            linux: self.linux,
            windows: self.windows,
            cdi_devices: self.cdi_devices,
            stop_signal: self.stop_signal,
        }
    }
}

// only support limit config now.
pub fn get_linux_container_config(
    res: Option<ContainerRes>,
) -> Result<Option<LinuxContainerConfig>, anyhow::Error> {
    if let Some(limits) = res.and_then(|r| r.limits) {
        Ok(Some(LinuxContainerConfig {
            resources: Some(parse_resource(limits.cpu, limits.memory)?),
            ..Default::default()
        }))
    } else {
        Ok(None)
    }
}

/// Convert CPU resource descriptions in the form of `1` or `1000m`,
/// and memory resource descriptions in the form of `1Mi`, `1Ki`, or `1Gi` to LinuxContainerResource.
fn parse_resource(
    cpu: Option<String>,
    memory: Option<String>,
) -> Result<LinuxContainerResources, anyhow::Error> {
    let mut res = LinuxContainerResources::default();

    if let Some(c) = cpu {
        let period: i64 = 1_000_000;
        let portion: i64 = if c.ends_with("m") {
            c[..c.len() - 1]
                .parse::<i64>()
                .map_err(|e| anyhow!("Failed to parse cpu resource config: {}", e))?
                * period
                / 1000
        } else {
            (c.parse::<f64>()
                .map_err(|e| anyhow!("Failed to parse cpu resource config: {}", e))?
                * period as f64) as i64
        };
        res.cpu_period = period;
        res.cpu_quota = portion;
    }

    if let Some(m) = memory {
        let mem_result: Result<i64, _> = if m.ends_with("Gi") {
            m[..m.len() - 2]
                .parse()
                .map(|x: i64| x * 1024 * 1024 * 1024)
        } else if m.ends_with("Mi") {
            m[..m.len() - 2].parse().map(|x: i64| x * 1024 * 1024)
        } else if m.ends_with("Ki") {
            m[..m.len() - 2].parse().map(|x: i64| x * 1024)
        } else {
            return Err(anyhow!("Failed to parse memory resource config: {}", m));
        };
        let mem =
            mem_result.map_err(|e| anyhow!("Failed to parse memory resource config: {}", e))?;
        res.memory_limit_in_bytes = mem;
    }

    Ok(res)
}

/// Convert type used to describe container config to oci_spec config.
impl From<&LinuxContainerResources> for LinuxResources {
    fn from(value: &LinuxContainerResources) -> Self {
        let mut res = LinuxResourcesBuilder::default();
        if value.cpu_period != 0 {
            res = res.cpu(
                LinuxCpuBuilder::default()
                    .period(value.cpu_period as u64)
                    .quota(value.cpu_quota)
                    .build()
                    .unwrap(),
            );
        }
        if value.memory_limit_in_bytes != 0 {
            res = res.memory(
                LinuxMemoryBuilder::default()
                    .limit(value.memory_limit_in_bytes)
                    .build()
                    .unwrap(),
            );
        }
        res.build().unwrap()
    }
}
