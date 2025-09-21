use json::JsonValue;
// Copyright (c) 2024 https://github.com/jokemanfire/rust-cni
use log::{debug, error};
use std::sync::Arc;

use crate::rust_cni::{
    api::{CNI, CNIConfig},
    config::ConfigFile,
    exec::RawExec,
    namespace::{Namespace, Network},
    result::APIResult,
    types::Config,
};

pub struct Libcni {
    config: Config,
    cni_interface: Arc<Box<dyn CNI + Send + Sync>>,
    network_count: i64,
    networks: Vec<Network>,
}

impl Default for Libcni {
    fn default() -> Self {
        Libcni {
            config: Config {
                plugin_dirs: vec!["/opt/cni/bin".to_string()],
                plugin_conf_dir: "/etc/cni/net.d".to_string(),
                plugin_max_conf_num: 1,
                prefix: "vethcni".to_string(),
            },
            cni_interface: Arc::new(Box::new(CNIConfig {
                path: vec!["/opt/cni/bin".to_string()],
                exec: RawExec::default(),
                cache_dir: "/var/lib/cni/cache".to_string(),
            })),
            network_count: 1,
            networks: Vec::default(),
        }
    }
}

impl Libcni {
    pub fn load_default_conf(&mut self) {
        debug!(
            "Loading default CNI configuration from {}",
            self.config.plugin_conf_dir
        );
        let extensions = vec![
            "conf".to_string(),
            "conflist".to_string(),
            "json".to_string(),
        ];

        match ConfigFile::config_files(self.config.plugin_conf_dir.clone(), extensions) {
            Ok(config_files) => {
                if config_files.is_empty() {
                    error!(
                        "No CNI configuration files found in {}",
                        self.config.plugin_conf_dir
                    );
                    return;
                }

                let mut networks = Vec::new();
                let mut cnt = 0;

                for configfile in config_files {
                    debug!("Processing CNI config file: {configfile}");
                    // Do not load more than plugin_max_conf_num
                    if cnt >= self.config.plugin_max_conf_num {
                        break;
                    }
                    if configfile.ends_with(".conflist") {
                        match ConfigFile::read_configlist_file(configfile.clone()) {
                            Some(config) => {
                                debug!("Loaded CNI network config: {}", config.name);
                                networks.push(Network {
                                    cni: self.cni_interface.clone(),
                                    config,
                                    ifname: self.config.prefix.clone() + &cnt.to_string(),
                                });
                            }
                            None => error!("Failed to read config list file: {configfile}"),
                        }
                    } else if configfile.ends_with(".conf") || configfile.ends_with(".json") {
                        match ConfigFile::read_config_file(configfile.clone()) {
                            Some(config) => {
                                debug!("Loaded CNI single config: {}", config.network.name);
                                // Convert single config to config list
                                let config_list = ConfigFile::convert_to_config_list(config);
                                networks.push(Network {
                                    cni: self.cni_interface.clone(),
                                    config: config_list,
                                    ifname: self.config.prefix.clone() + &cnt.to_string(),
                                });
                            }
                            None => error!("Failed to read config file: {configfile}"),
                        }
                    }
                    cnt += 1;
                }

                self.networks = networks;
                self.network_count = cnt;
                debug!("Loaded {} CNI networks", self.network_count);
            }
            Err(e) => {
                error!("Failed to read CNI config files: {e}");
            }
        }
    }

    pub fn new(
        plugin_dirs: Option<Vec<String>>,
        conf_dir: Option<String>,
        cache_dir: Option<String>,
    ) -> Self {
        debug!("Creating new CNI instance");
        let plugin_dirs = plugin_dirs.unwrap_or(vec!["/opt/cni/bin".to_string()]);
        let conf_dir = conf_dir.unwrap_or("/etc/cni/net.d".to_string());
        let cache_dir = cache_dir.unwrap_or("/var/lib/cni/cache".to_string());
        Libcni {
            config: Config {
                plugin_dirs: plugin_dirs.clone(),
                plugin_conf_dir: conf_dir.clone(),
                plugin_max_conf_num: 1,
                prefix: "vethcni".to_string(),
            },
            cni_interface: Arc::new(Box::new(CNIConfig {
                path: plugin_dirs.clone(),
                exec: RawExec::default(),
                cache_dir: cache_dir.clone(),
            })),
            network_count: 1,
            networks: Vec::default(),
        }
    }

    pub fn load(
        &mut self,
        conf_dir: Option<String>,
        plugin_dirs: Option<Vec<String>>,
    ) -> Result<(), String> {
        debug!("Loading custom CNI configuration");

        if let Some(conf_dir) = conf_dir {
            self.config.plugin_conf_dir = conf_dir;
        }

        if let Some(plugin_dirs) = plugin_dirs {
            self.config.plugin_dirs = plugin_dirs;

            // Update CNI interface with new plugin paths
            self.cni_interface = Arc::new(Box::new(CNIConfig {
                path: self.config.plugin_dirs.clone(),
                exec: RawExec::default(),
                cache_dir: String::default(),
            }));
        }

        self.load_default_conf();
        Ok(())
    }

    pub fn add_lo_network(&mut self) -> Result<(), String> {
        debug!("Adding loopback network configuration");
        let datas = r#"{
            "cniVersion": "0.3.1",
            "name": "cni-loopback",
            "plugins": [{
              "type": "loopback"
            }]
        }"#
        .to_string();

        match ConfigFile::config_from_bytes(datas.as_bytes()) {
            Ok(loconfig) => {
                debug!("Loopback network configuration added");
                self.networks.push(Network {
                    cni: self.cni_interface.clone(),
                    config: loconfig,
                    ifname: "lo".to_string(),
                });
                Ok(())
            }
            Err(e) => {
                error!("Failed to add loopback network: {e}");
                Err(format!("Can't add lo network: {e}"))
            }
        }
    }

    pub fn status(&self) -> Result<(), String> {
        debug!(
            "Checking CNI status, networks count: {}",
            self.networks.len()
        );
        if self.networks.len() < self.network_count as usize {
            error!(
                "CNI not properly initialized: expected {} networks, found {}",
                self.network_count,
                self.networks.len()
            );
            return Err("CNI not properly initialized".to_string());
        }
        Ok(())
    }

    pub fn get_networks(&self) -> &Vec<Network> {
        &self.networks
    }

    pub fn setup(&self, id: String, path: String) -> Result<JsonValue, String> {
        debug!("Setting up networks for container: {id}");

        // Check status
        self.status()?;

        // Create namespace
        let namespace = Namespace::new(id.clone(), path.clone());

        // Attach networks
        let results = self.attach_networks(&namespace)?;

        let result_json = results[0].get_json();

        debug!("Networks setup completed for container: {id}");
        Ok(result_json)
    }

    pub fn remove(&self, id: String, path: String) -> Result<(), String> {
        debug!("Removing networks for container: {id}");

        // Check status
        self.status()?;

        // Create namespace
        let namespace = Namespace::new(id.clone(), path.clone());

        // Remove networks
        let mut errors = Vec::new();
        for net in &self.networks {
            match net.remove(&namespace) {
                Ok(_) => debug!("Removed network {} for container {}", net.config.name, id),
                Err(e) => {
                    let err_msg = format!(
                        "Failed to remove network {} for container {}: {}",
                        net.config.name, id, e
                    );
                    errors.push(err_msg);
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }

        debug!("Networks removal completed for container: {id}");
        Ok(())
    }

    pub fn check(&self, id: String, path: String) -> Result<(), String> {
        debug!("Checking networks for container: {id}");

        // Check status
        self.status()?;

        // Create namespace
        let namespace = Namespace::new(id.clone(), path.clone());

        // Check networks
        let mut errors = Vec::new();
        for net in &self.networks {
            match net.check(&namespace) {
                Ok(_) => debug!(
                    "Network {} is correctly configured for container {}",
                    net.config.name, id
                ),
                Err(e) => {
                    let err_msg = format!(
                        "Network {} check failed for container {}: {}",
                        net.config.name, id, e
                    );
                    errors.push(err_msg);
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        debug!("Networks check completed for container: {id}");
        Ok(())
    }

    fn attach_networks(&self, ns: &Namespace) -> Result<Vec<Box<dyn APIResult>>, String> {
        debug!("Attaching {} networks", self.networks.len());

        let mut errors = Vec::new();
        let mut results = Vec::new();

        for net in &self.networks {
            match net.attach(ns) {
                Ok(result) => {
                    debug!("Attached network {} successfully", net.config.name);
                    results.push(result);
                }
                Err(e) => {
                    let err_msg = format!("Failed to attach network {} : {}", net.config.name, e);
                    error!("{err_msg}");
                    errors.push(err_msg);
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("; "));
        }

        Ok(results)
    }
}
