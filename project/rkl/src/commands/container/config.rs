use anyhow::{Ok, Result};
use std::collections::HashMap;

use crate::{
    cri::cri_api::{
        CdiDevice, ContainerConfig, ContainerMetadata, Device, ImageSpec, KeyValue,
        LinuxContainerConfig, Mount, WindowsContainerConfig,
    },
    task::{ContainerSpec, get_linux_container_config},
};

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
            mounts: vec![
                Mount {
                    container_path: "/proc".to_string(),
                    host_path: "proc".to_string(),
                    readonly: false,
                    selinux_relabel: false,
                    propagation: 0,
                    uid_mappings: vec![],
                    gid_mappings: vec![],
                    recursive_read_only: false,
                    image: None,
                    image_sub_path: "".to_string(),
                },
                Mount {
                    container_path: "/dev".to_string(),
                    host_path: "tmpfs".to_string(),
                    readonly: false,
                    selinux_relabel: false,
                    propagation: 0,
                    uid_mappings: vec![],
                    gid_mappings: vec![],
                    recursive_read_only: false,
                    image: None,
                    image_sub_path: "".to_string(),
                },
            ],
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
    pub fn from_container_spec(&mut self, spec: ContainerSpec) -> Result<&mut Self> {
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
        let args = spec.args.clone();

        self.metadata = metadata;
        self.image = image;
        self.log_path = log_path;
        self.linux = linux;
        self.args = Some(args);

        Ok(self)
    }

    pub fn mounts(&mut self, mounts: Vec<Mount>) -> &mut Self {
        self.mounts.extend(mounts);
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
