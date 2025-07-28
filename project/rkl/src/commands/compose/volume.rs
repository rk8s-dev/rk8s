use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Ok;
use anyhow::Result;
use anyhow::anyhow;

use crate::commands::compose::spec::ComposeSpec;
use crate::cri::cri_api::Mount;

/// pattern like this "<host_path>:<container_path>:ro" read-only
/// pattern like this "<host_path>:<container_path>:rw" read-write
///
/// "/opt/era:/mnt/run/tmp"
pub struct VolumePattern {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

impl Default for VolumeManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VolumeManager {
    // storage the key-value <volume_name>:<specific_path>
    pub volumes: HashMap<String, String>,
}

impl VolumeManager {
    pub fn string_to_pattern(volumes: Vec<String>) -> Result<Vec<VolumePattern>> {
        volumes
            .into_iter()
            .map(|v| {
                let parts: Vec<&str> = v.split(":").collect();
                let (host_path, container_path, read_only) = match parts.len() {
                    2 => (parts[0], parts[1], ""),
                    3 => (parts[0], parts[1], parts[2]),
                    _ => return Err(anyhow!("Invalid volumes mapping syntax in compose file")),
                };
                // validate the read_only str
                if !read_only.is_empty() && !read_only.eq("ro") {
                    return Err(anyhow!("Invalid volumes mapping syntax in compose file"));
                }

                Ok(VolumePattern {
                    host_path: host_path.to_string(),
                    container_path: container_path.to_string(),
                    read_only: !read_only.is_empty(),
                })
            })
            .collect()
    }

    pub fn map_to_mount(volumes: Vec<String>) -> Result<Vec<Mount>> {
        let mapped_volumes = VolumeManager::string_to_pattern(volumes)?;
        mapped_volumes
            .into_iter()
            .map(|srv_v| {
                // validate the hostpath and the container path
                // if the hostpath is not exist create one
                let path = Path::new(&srv_v.host_path);
                if !path.exists() {
                    fs::create_dir_all(path)?;
                }
                Ok(Mount {
                    container_path: srv_v.container_path,
                    host_path: srv_v.host_path,
                    readonly: srv_v.read_only,
                    selinux_relabel: false,
                    propagation: 0,
                    uid_mappings: vec![],
                    gid_mappings: vec![],
                    recursive_read_only: false,
                    image: None,
                    image_sub_path: "".to_string(),
                })
            })
            .collect()
    }

    pub fn handle(&mut self, _: &ComposeSpec) -> Result<()> {
        // TODO:  implements the top-volume definition
        // if let Some(volumes) = spec.volumes {}
        Ok(())
    }
    pub fn new() -> Self {
        Self {
            volumes: HashMap::new(),
        }
    }
}
