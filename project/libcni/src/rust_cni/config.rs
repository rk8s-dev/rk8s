// Copyright (c) 2024 https://github.com/divinerapier/cni-rs
use log::{debug, error, trace, warn};
use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use super::{
    api::NetworkConfigList,
    types::{NetConf, NetworkConfig},
};

pub struct ConfigFile {}

impl ConfigFile {
    pub fn config_files(dir: String, extensions: Vec<String>) -> Result<Vec<String>, String> {
        debug!("Reading CNI config files from directory: {dir}");
        let mut conf_files = Vec::default();

        match fs::read_dir(&dir) {
            Ok(dir_entries) => {
                for entry in dir_entries {
                    match entry {
                        Ok(file) => {
                            let file_path = file.path();
                            if let Some(ext) = file_path.extension() {
                                let ext_str = ext.to_string_lossy().to_string();
                                if extensions.contains(&ext_str) {
                                    let path_str: String =
                                        file.path().to_string_lossy().to_string();
                                    trace!("Found config file: {path_str}");
                                    conf_files.push(path_str);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error reading directory entry: {e}");
                        }
                    }
                }

                debug!("Found {} config files", conf_files.len());
                Ok(conf_files)
            }
            Err(e) => {
                error!("Failed to read CNI config directory {dir}: {e}");
                Err(format!("Failed to read directory {dir}: {e}"))
            }
        }
    }

    pub fn config_from_bytes(datas: &[u8]) -> Result<NetworkConfigList, String> {
        trace!("Parsing CNI config from bytes: {} bytes", datas.len());

        match serde_json::from_slice::<serde_json::Value>(datas) {
            Ok(ncmaps) => {
                // Extract required fields
                let name = match ncmaps.get("name") {
                    Some(name_val) => match name_val.as_str() {
                        Some(name_str) => name_str.to_string(),
                        None => return Err("'name' is not a string".to_string()),
                    },
                    None => return Err("'name' field is required".to_string()),
                };

                let version = match ncmaps.get("cniVersion") {
                    Some(ver_val) => match ver_val.as_str() {
                        Some(ver_str) => ver_str.to_string(),
                        None => return Err("'cniVersion' is not a string".to_string()),
                    },
                    None => return Err("'cniVersion' field is required".to_string()),
                };

                let mut disable_check = false;
                if let Some(check) = ncmaps.get("disableCheck")
                    && let Some(check_bool) = check.as_bool()
                {
                    disable_check = check_bool;
                }

                let mut ncflist = NetworkConfigList::default();
                let mut all_plugins = Vec::new();

                if let Some(plugins) = ncmaps.get("plugins") {
                    if let Some(plugins_arr) = plugins.as_array() {
                        for plugin in plugins_arr {
                            let string_plugin = plugin.to_string();
                            let plg_bytes = string_plugin.as_bytes().to_vec();

                            match serde_json::from_str::<NetConf>(&string_plugin) {
                                Ok(tmp) => {
                                    trace!("Parsed plugin: {}", tmp._type);
                                    all_plugins.push(NetworkConfig {
                                        network: tmp,
                                        bytes: plg_bytes,
                                    });
                                }
                                Err(e) => {
                                    error!("Failed to parse plugin config: {e}");
                                    return Err(format!("Invalid plugin config: {e}"));
                                }
                            }
                        }
                    } else {
                        return Err("'plugins' must be an array".to_string());
                    }
                } else {
                    return Err("'plugins' field is required".to_string());
                }

                ncflist.name = name;
                ncflist.cni_version = version;
                ncflist.bytes = datas.to_vec();
                ncflist.disable_check = disable_check;
                ncflist.plugins = all_plugins;
                debug!("Successfully parsed NetworkConfigList: {}", ncflist.name);
                Ok(ncflist)
            }
            Err(e) => {
                error!("Failed to parse CNI config: {e}");
                Err(format!("Invalid JSON: {e}"))
            }
        }
    }

    pub fn read_configlist_file(file_path: String) -> Option<NetworkConfigList> {
        debug!("Reading CNI config list from file: {file_path}");
        let path = Path::new(&file_path);

        if !path.exists() {
            error!("Config file does not exist: {file_path}");
            return None;
        }

        match File::open(path) {
            Ok(file) => {
                let mut file_bytes = Vec::default();
                let mut reader = BufReader::new(file);

                match reader.read_to_end(&mut file_bytes) {
                    Ok(_) => match Self::config_from_bytes(&file_bytes) {
                        Ok(ncflist) => {
                            debug!("Successfully read config list: {}", ncflist.name);
                            Some(ncflist)
                        }
                        Err(e) => {
                            error!("Failed to parse config list: {e}");
                            None
                        }
                    },
                    Err(e) => {
                        error!("Failed to read config file {file_path}: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to open config file {file_path}: {e}");
                None
            }
        }
    }

    pub fn read_config_file(file_path: String) -> Option<NetworkConfig> {
        debug!("Reading CNI single config from file: {file_path}");
        let path = Path::new(&file_path);

        if !path.exists() {
            error!("Config file does not exist: {file_path}");
            return None;
        }

        match File::open(path) {
            Ok(file) => {
                let mut file_bytes = Vec::default();
                let mut reader = BufReader::new(file);

                match reader.read_to_end(&mut file_bytes) {
                    Ok(_) => match serde_json::from_slice::<NetConf>(&file_bytes) {
                        Ok(net_conf) => {
                            debug!("Successfully read config: {net_conf:?}");
                            Some(NetworkConfig {
                                network: net_conf,
                                bytes: file_bytes,
                            })
                        }
                        Err(e) => {
                            error!("Failed to parse config: {e}");
                            None
                        }
                    },
                    Err(e) => {
                        error!("Failed to read config file {file_path}: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to open config file {file_path}: {e}");
                None
            }
        }
    }

    pub fn convert_to_config_list(config: NetworkConfig) -> NetworkConfigList {
        debug!(
            "Converting single config to config list: {}",
            config.network.name
        );
        NetworkConfigList {
            name: config.network.name.clone(),
            cni_version: config.network.cni_version.clone(),
            disable_check: false,
            plugins: vec![config],
            bytes: Vec::new(), // This will be empty for converted configs
        }
    }
}
