use crate::commands::compose::spec::{ComposeSpec, ConfigsSpec};
use std::collections::HashMap;

use crate::cri::cri_api::Mount;

#[derive(Clone, Debug)]
pub struct ConfigMountInfo {
    pub container_path: String,
    pub host_path: String,
    pub read_only: bool,
}

pub struct ConfigManager {
    pub configs_map: HashMap<String, ConfigsSpec>,
    service_config_mounts: HashMap<String, Vec<ConfigMountInfo>>,
}

impl ConfigManager {
    pub fn new() -> Self {
        Self {
            configs_map: HashMap::new(),
            service_config_mounts: HashMap::new(),
        }
    }

    /// 处理 ComposeSpec，填充 configs_map 和每个 service 的 config 映射 mount 信息
    pub fn handle(&mut self, spec: &ComposeSpec) {
        if let Some(configs) = &spec.configs {
            self.configs_map = configs.clone();
        }

        for (srv_name, srv_spec) in &spec.services {
            let mut mounts = vec![];

            if let Some(srv_configs) = &srv_spec.configs {
                for cfg in srv_configs {
                    let source = &cfg.source;
                    let target = &cfg.target;

                    let config_file = self
                        .configs_map
                        .get(source)
                        .map(|c| c.file.clone())
                        .unwrap_or_default();

                    if config_file.is_empty() {
                        tracing::warn!("Empty config file for source '{}', skipping...", source);
                        continue;
                    }

                    mounts.push(ConfigMountInfo {
                        container_path: target.clone(),
                        host_path: config_file,
                        read_only: true,
                    });
                }
            }

            if !mounts.is_empty() {
                self.service_config_mounts.insert(srv_name.clone(), mounts);
            }
        }
    }

    pub fn get_mounts_by_service(&self, service: &str) -> Vec<Mount> {
        self.service_config_mounts
            .get(service)
            .unwrap_or(&vec![])
            .iter()
            .map(|m| Mount {
                container_path: m.container_path.clone(),
                host_path: m.host_path.clone(),
                readonly: m.read_only,
                selinux_relabel: false,
                propagation: 0,
                uid_mappings: vec![],
                gid_mappings: vec![],
                recursive_read_only: false,
                image: None,
                image_sub_path: "".to_string(),
            })
            .collect()
    }

    pub fn validate(&mut self) {}
}
