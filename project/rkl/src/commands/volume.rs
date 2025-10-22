use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::anyhow;
use anyhow::{Ok, Result};
use clap::{ArgAction, Subcommand};
use libcontainer::container::State;
use rand::RngCore;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Write as _;
use std::io::Write;
use tabwriter::TabWriter;
use tracing::debug;

use crate::commands::{compose::ComposeMetadata, utils::parse_key_val};
use crate::cri::cri_api::Mount;

#[derive(Debug)]
pub enum PatternType {
    Anonymous,
    BindMount,
    Named,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum MountType {
    Bind,
    Nfs,
    Tmpfs,
    Cifs,
}

/// pattern like this "<host_path>:<container_path>:ro" read-only
/// pattern like this "<host_path>:<container_path>:rw" read-write
///
/// "/opt/era:/mnt/run/tmp"
#[derive(Debug)]
pub struct VolumePattern {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
    pub pattern_type: PatternType,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeMetadata {
    pub name: String,
    pub driver: String,
    pub mountpoint: PathBuf,
    pub created_at: String,
    pub labels: HashMap<String, String>,
    pub options: HashMap<String, String>,
    pub scope: String, // "local" or "global"
    pub status: HashMap<String, String>,
    pub reference: Vec<String>, // the containers which uses this volume
}

#[allow(dead_code)]
pub enum Driver {
    Local,
    // TODO: Support cloud driver
    Azure,
    Rexray,
}

#[derive(Subcommand)]
pub enum VolumeCommand {
    #[command(about = "Create a volume")]
    Create {
        #[arg(value_name = "VOLUME_NAME")]
        name: String,
        #[arg(long, short = 'd')]
        driver: Option<String>,
        #[arg(long, short = 'o', value_parser=parse_key_val)]
        opts: Option<Vec<(String, String)>>,
    },

    #[command(about = "Remove one or more volumes")]
    Rm {
        volumes: Vec<String>,
        #[arg(long, short = 'f', action=ArgAction::SetTrue)]
        force: bool,
    },

    #[command(about = "List volumes")]
    Ls {
        #[arg(long, short = 'q', action=ArgAction::SetTrue)]
        quiet: bool,
    },

    #[command(about = "Display detailed information on one or more volumes")]
    Inspect { name: Vec<String> },

    #[command(about = "Remove all unused local volumes")]
    Prune {
        #[arg(long, short = 'f', action=ArgAction::SetTrue)]
        force: bool,
    },
}

pub struct VolumeManager {
    volume_root: PathBuf,   // /var/lib/rkl/volumes
    metadata_path: PathBuf, // /var/lib/rkl/volumes/metadata.json
    volumes: HashMap<String, VolumeMetadata>,
}

impl VolumeManager {
    pub fn new() -> Result<Self> {
        let volume_root = PathBuf::from("/var/lib/rkl/volumes");
        let metadata_path = volume_root.join("metadata.json");

        fs::create_dir_all(&volume_root)?;

        let volumes = if metadata_path.exists() {
            Self::load_metadata(&metadata_path)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            volume_root,
            metadata_path,
            volumes,
        })
    }

    /// This function used to handle the container's volumes
    /// parse the VolumePattern like "<host_path>:<container_path>:ro" directly to cri::Mount.
    /// And return two things:
    /// 1. Vec<Mount>
    /// 2. Vec<String> the volume name array
    ///
    /// is_compose: if this volume is from compose, then when this volume is named, this volume will not be created
    /// here.(Because it's created when compose parse the compose_spec)
    pub fn handle_container_volume(
        &mut self,
        parsed_pattern: Vec<VolumePattern>,
        is_compose: bool,
    ) -> Result<(Vec<String>, Vec<Mount>)> {
        let mut mounts: Vec<Mount> = vec![];
        let mut volume_names: Vec<String> = vec![];
        for pattern in parsed_pattern {
            let mut mount = Mount {
                container_path: pattern.container_path.clone(),
                host_path: "".to_string(),
                readonly: false,
                selinux_relabel: false,
                propagation: 0,
                uid_mappings: vec![],
                gid_mappings: vec![],
                recursive_read_only: false,
                image: None,
                image_sub_path: "".to_string(),
            };

            let mut volume_name = pattern.host_path.clone();

            debug!("get volume pattern: {pattern:?}");

            match pattern.pattern_type {
                PatternType::Anonymous => {
                    let name = generate_anonymous_volume_name();
                    let resp = self.create_(name.clone(), None, HashMap::new())?;
                    mount.host_path = resp.mountpoint.to_str().unwrap().to_string();
                    volume_name = name;
                }
                PatternType::BindMount => {
                    mount.host_path = pattern.host_path.clone();
                }
                PatternType::Named => {
                    volume_name = pattern.host_path.clone();

                    // for compose if there is a undefined volume. then return Error
                    if is_compose && !self.volumes.contains_key(&volume_name) {
                        return Err(anyhow!("{} is not defined in compose spec", volume_name));
                    }

                    // for single container if this named volume is not exists create it automatically
                    if !is_compose && !self.volumes.contains_key(&volume_name) {
                        let _ = self.create_(volume_name.clone(), None, HashMap::new())?;
                    }
                    mount.host_path = self.get_mountpoint_from_name(&volume_name)?;
                }
            };
            mount.container_path = pattern.container_path;
            mount.readonly = pattern.read_only;
            mounts.push(mount);
            volume_names.push(volume_name);
        }
        Ok((volume_names, mounts))
    }

    pub fn get_mountpoint_from_name(&self, name: &str) -> Result<String> {
        // TODO: handle does not exist situation
        // Ok(self.volumes.get(name).ok_or_else(|| format!("the volume name does not exist"))?.mountpoint.to_str().unwrap().to_string())
        Ok(self
            .volumes
            .get(name)
            .unwrap()
            .mountpoint
            .to_str()
            .unwrap()
            .to_string())
    }

    pub fn create_(
        &mut self,
        name: String,
        driver: Option<String>,
        opts: HashMap<String, String>,
    ) -> Result<VolumeMetadata> {
        if self.volumes.contains_key(&name) {
            return Err(anyhow!("volume {} already exists", name));
        }

        let driver = driver.unwrap_or_else(|| "local".to_string());
        let mountpoint = self.volume_root.join(&name).join("_data");

        fs::create_dir_all(&mountpoint)?;

        let metadata = VolumeMetadata {
            name: name.clone(),
            driver,
            mountpoint,
            created_at: chrono::Utc::now().to_rfc3339(),
            labels: HashMap::new(),
            options: opts,
            scope: "local".to_string(),
            status: HashMap::new(),
            reference: vec![],
        };

        self.volumes.insert(name, metadata.clone());
        self.save_metadata()?;

        Ok(metadata)
    }

    pub fn remove_(&mut self, name: &str, force: bool) -> Result<()> {
        let volume = self
            .volumes
            .get(name)
            .ok_or_else(|| anyhow!("volume {} not found", name))?;

        if !force && self.is_volume_in_use(name)? {
            return Err(anyhow!("volume {} is in use", name));
        }

        println!("{}", name);

        fs::remove_dir_all(volume.mountpoint.parent().unwrap())?;
        self.volumes.remove(name);
        self.save_metadata()?;

        Ok(())
    }

    pub fn list(&self) -> Vec<&VolumeMetadata> {
        self.volumes.values().collect()
    }

    pub fn inspect_(&self, name: &str) -> Result<&VolumeMetadata> {
        self.volumes
            .get(name)
            .ok_or_else(|| anyhow!("volume {} not found", name))
    }

    pub fn prune_(&mut self, force: bool) -> Result<Vec<String>> {
        let mut removed = Vec::new();
        let names: Vec<String> = self.volumes.keys().cloned().collect();
        if names.is_empty() {
            return Ok(vec![]);
        }

        for name in names {
            self.remove_(&name, force)?;
            removed.push(name);
        }

        Ok(removed)
    }

    /// scan all the container's state json
    /// check if there is container refer this volume
    fn is_volume_in_use(&self, name: &str) -> Result<bool> {
        let root_path = PathBuf::from_str("/run/youki")?;
        for entry in fs::read_dir(root_path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                // TODO: Hard code "compose", which means there is no container can be named as "compose"
                if entry.file_name().to_str().unwrap() != "compose" {
                    let content = fs::read_to_string(entry.path().join("state.json"))?;
                    let container_state: State = serde_json::from_str(&content)?;
                    if let Some(volumes) = container_state.volumes
                        && volumes.contains(&name.to_string())
                    {
                        return Ok(true);
                    }
                }
            }
        }

        // Compose
        let root_path = PathBuf::from_str("/run/youki/compose")?;
        for dir_entry in fs::read_dir(root_path)? {
            let dir_entry = dir_entry?;
            if dir_entry.metadata().unwrap().is_dir() {
                let path = dir_entry.path().join("metadata.json");
                if path.exists() {
                    let content = fs::read_to_string(path).map_err(|e| {
                        anyhow!(
                            "failed to read compose {:?} metada.json error: {e:?}",
                            dir_entry.file_name(),
                        )
                    })?;
                    let metadata: ComposeMetadata = serde_json::from_str(&content)?;
                    if metadata.volumes.contains(&name.to_string()) {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    fn save_metadata(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.volumes)?;
        fs::write(&self.metadata_path, json)?;
        Ok(())
    }

    fn load_metadata(path: &Path) -> Result<HashMap<String, VolumeMetadata>> {
        let content = fs::read_to_string(path)?;
        // cache reference
        load_volume_container_reference(serde_json::from_str(&content)?)
    }

    // ========Command entrypoints========
    fn create(
        &mut self,
        name: String,
        driver: Option<String>,
        opts: Option<Vec<(String, String)>>,
    ) -> Result<()> {
        let opts = opts.unwrap_or_default().into_iter().collect();
        let metadata = self.create_(name, driver, opts)?;
        println!("{}", metadata.name);
        Ok(())
    }

    fn rm(&mut self, names: Vec<String>, force: bool) -> Result<()> {
        for name in names {
            self.remove_(name.as_str(), force)?;
        }
        Ok(())
    }

    fn ls(&self, quiet: bool) -> Result<()> {
        let volumes = self.list();
        let mut content = String::new();
        for v in volumes {
            if !quiet {
                let _ = writeln!(content, "{}\t{}", v.driver, v.name);
            } else {
                let _ = writeln!(content, "{}", v.name);
            }
        }

        let mut tab_writer = TabWriter::new(io::stdout());
        if !quiet {
            writeln!(&mut tab_writer, "DRIVER\tVOLUME NAME")?;
        } else {
            writeln!(&mut tab_writer, "VOLUME NAME")?;
        }
        write!(&mut tab_writer, "{content}")?;
        tab_writer.flush()?;

        Ok(())
    }
    fn inspect(&self, names: Vec<String>) -> Result<()> {
        for name in names {
            let meta = self.inspect_(name.as_str())?;
            let meta_str = serde_json::to_string_pretty(meta)?;
            println!("{meta_str}");
        }
        Ok(())
    }
    fn prune(&mut self, force: bool) -> Result<()> {
        self.prune_(force).and_then(|_| Ok(()))
    }
}

/// scan the container's state and update the metadata struct
fn load_volume_container_reference(
    content: HashMap<String, VolumeMetadata>,
) -> Result<HashMap<String, VolumeMetadata>> {
    // TODO:
    Ok(content)
}

/// Generate the anonymous name using random bytes
fn generate_anonymous_volume_name() -> String {
    let mut bytes = [0u8; 32]; // 32 bytes = 64 hex chars
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn string_to_pattern(v: &str) -> Result<VolumePattern> {
    let parts: Vec<&str> = v.split(":").collect();

    debug!("[string_to_pattern] get volume string: {v:?}  get parts: {parts:?}");

    let mut typ = PatternType::BindMount;
    let (host_path, container_path, read_only) = match parts.len() {
        1 => ("", parts[0], ""),
        2 => (parts[0], parts[1], ""),
        3 => (parts[0], parts[1], parts[2]),
        _ => return Err(anyhow!("Invalid volumes mapping syntax in compose file")),
    };
    // validate the read_only str
    if !read_only.is_empty() && !read_only.eq("ro") {
        return Err(anyhow!("Invalid volumes mapping syntax in compose file"));
    }

    if host_path.is_empty() {
        typ = PatternType::Anonymous;
    } else if !host_path.contains('/') {
        typ = PatternType::Named;
    }

    Ok(VolumePattern {
        host_path: host_path.to_string(),
        container_path: container_path.to_string(),
        read_only: !read_only.is_empty(),
        pattern_type: typ,
    })
}

pub fn volume_execute(cmd: VolumeCommand) -> Result<()> {
    let mut v_manager = VolumeManager::new()?;
    match cmd {
        VolumeCommand::Create { name, driver, opts } => v_manager.create(name, driver, opts),
        VolumeCommand::Rm { volumes, force } => v_manager.rm(volumes, force),
        VolumeCommand::Ls { quiet } => v_manager.ls(quiet),
        VolumeCommand::Inspect { name } => v_manager.inspect(name),
        VolumeCommand::Prune { force } => v_manager.prune(force),
    }
}
