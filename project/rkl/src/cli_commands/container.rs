use crate::{
    commands::create,
    cri::cri_api::{
        ContainerConfig, ContainerMetadata, CreateContainerResponse, ImageSpec, KeyValue, Mount,
    },
    rootpath,
    task::{ContainerSpec, add_cap_net_raw, get_linux_container_config},
};
use anyhow::{Ok, Result, anyhow};
use liboci_cli::Create;
use oci_spec::runtime::{
    LinuxBuilder, LinuxNamespaceBuilder, LinuxNamespaceType, ProcessBuilder, Spec,
};
use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    path::Path,
};

struct ContainerRunner {
    sepc: ContainerSpec,
    config: Option<ContainerConfig>,
}

impl ContainerRunner {
    pub fn from_file(spec_path: &str) -> Result<Self> {
        // read the container_spec bytes
        let mut file = File::open(spec_path)
            .map_err(|e| anyhow!("open the container spec file failed: {e}"))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let container_spec: ContainerSpec = serde_yaml::from_str(&content)?;
        Ok(ContainerRunner {
            sepc: container_spec,
            config: None,
        })
    }

    pub fn build_config(&mut self) -> Result<()> {
        let config = ContainerConfig {
            metadata: Some(ContainerMetadata {
                name: self.sepc.name.clone(),
                attempt: 0,
            }),
            image: Some(ImageSpec {
                image: self.sepc.image.clone(),
                annotations: std::collections::HashMap::new(),
                user_specified_image: self.sepc.image.clone(),
                runtime_handler: String::new(),
            }),
            command: vec!["bin/sh".to_string()],
            args: self.sepc.args.clone(),
            working_dir: String::from("/"),
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
            labels: std::collections::HashMap::new(),
            annotations: std::collections::HashMap::new(),
            log_path: format!("{}/0.log", self.sepc.name),
            stdin: false,
            stdin_once: false,
            tty: false,
            linux: get_linux_container_config(self.sepc.resources.clone())?,
            windows: None,
            cdi_devices: vec![],
            stop_signal: 0,
        };
        self.config = std::option::Option::Some(config);

        Ok(())
    }

    pub fn create_container(&self) -> Result<CreateContainerResponse> {
        // validate the config
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("Container's Config is required"))?;

        let container_id = config
            .metadata
            .as_ref()
            .map(|data| data.name.clone())
            .ok_or_else(|| anyhow!("Container's metadata is required"))?;

        // 3. create oci spec
        let mut spec = Spec::default();

        // use the default namesapce configuration
        let namespaces = vec![
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Pid)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Network)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Ipc)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Uts)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Mount)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Cgroup)
                .build()?,
        ];

        let mut linux = LinuxBuilder::default().namespaces(namespaces);
        if let Some(x) = &config.linux {
            if let Some(r) = &x.resources {
                linux = linux.resources(r);
            }
        }
        let linux = linux.build()?;
        spec.set_linux(Some(linux));

        // build the procss path
        let mut process = ProcessBuilder::default()
            .args(self.sepc.args.clone())
            .build()?;

        let mut capabilities = process.capabilities().clone().unwrap();
        // add the CAP_NET_RAW
        add_cap_net_raw(&mut capabilities);
        process.set_capabilities(Some(capabilities));
        spec.set_process(Some(process));

        // 4. create a config.path at the bundle path
        let bundle_path = self.sepc.image.clone();
        if bundle_path.is_empty() {
            return Err(anyhow!("Bundle path (image) is empty"));
        }
        let bundle_dir = Path::new(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }

        let config_path = format!("{}/config.json", bundle_path);
        if Path::new(&config_path).exists() {
            std::fs::remove_file(&config_path).map_err(|e| {
                anyhow!(
                    "Failed to remove existing config.json in bundle path: {}",
                    e
                )
            })?;
        }
        let file = File::create(config_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &spec)?;
        writer.flush()?;

        // 5. get the create_args
        let create_args = Create {
            bundle: bundle_path.clone().into(),
            console_socket: None,
            pid_file: None,
            no_pivot: false,
            no_new_keyring: false,
            preserve_fds: 0,
            container_id: container_id.clone(),
        };

        // 6. get the root_path
        let root_path = rootpath::determine(None)
            .map_err(|e| anyhow!("Failed to determin the rootpath: {}", e))?;

        // 7. create the container return the container id
        create::create(create_args, root_path, false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        return Ok(CreateContainerResponse {
            container_id: container_id,
        });
    }
}

pub fn run_container(path: &str) -> Result<(), anyhow::Error> {
    // read the container_spec bytes to container_spec struct
    let mut runner = ContainerRunner::from_file(path)?;

    // create container_config
    runner.build_config()?;

    // create container
    runner.create_container()?;

    println!("{path}");
    Ok(())
}
