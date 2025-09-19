// Copyright (c) 2024 https://github.com/divinerapier/cni-rs
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

use super::exec::{Exec, ExecArgs, RawExec};
use crate::rust_cni::{
    error::CNIError,
    result::{APIResult, ResultCNI, result100},
    types::NetworkConfig,
};

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

pub trait CNI {
    fn add_network_list(
        &self,
        net: NetworkConfigList,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>>;

    fn check_network_list(&self, net: NetworkConfigList, rt: RuntimeConf) -> ResultCNI<()>;

    fn delete_network_list(&self, net: NetworkConfigList, rt: RuntimeConf) -> ResultCNI<()>;

    fn get_network_list_cached_result(
        &self,
        net: NetworkConfigList,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>>;

    fn add_network(
        &self,
        name: String,
        cni_version: String,
        net: NetworkConfig,
        prev_result: Option<Box<dyn APIResult>>,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>>;

    fn check_network(
        &self,
        name: String,
        cni_version: String,
        prev_result: Option<Box<dyn APIResult>>,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<()>;

    fn delete_network(
        &self,
        name: String,
        cni_version: String,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<()>;

    fn get_network_cached_result(
        &self,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>>;

    fn get_network_cached_config(
        &self,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<(Vec<u8>, RuntimeConf)>;

    fn validate_network_list(&self, net: NetworkConfigList) -> ResultCNI<Vec<String>>;

    fn validate_network(&self, net: NetworkConfig) -> ResultCNI<Vec<String>>;
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct NetworkConfigList {
    pub name: String,
    pub cni_version: String,
    pub disable_check: bool,
    pub plugins: Vec<NetworkConfig>,
    pub bytes: Vec<u8>,
}

impl NetworkConfigList {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Network name cannot be empty".to_string());
        }
        if self.cni_version.is_empty() {
            return Err("CNI version cannot be empty".to_string());
        }
        if self.plugins.is_empty() {
            return Err("At least one plugin is required".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConf {
    pub container_id: String,
    pub net_ns: String,
    pub if_name: String,
    pub args: Vec<[String; 2]>,
    pub capability_args: HashMap<String, String>,
    pub cache_dir: String,
}

impl RuntimeConf {
    pub fn get_cache_key(&self) -> String {
        let id_part = if self.container_id.len() > 12 {
            &self.container_id[..12]
        } else {
            &self.container_id
        };

        format!("{}-{}", id_part, self.if_name)
    }
}

#[derive(Default)]
pub struct CNIConfig {
    pub path: Vec<String>,
    pub exec: RawExec,
    pub cache_dir: String,
}

impl CNIConfig {
    fn get_cache_dir(&self, netname: &str) -> std::path::PathBuf {
        let cache_dir = if self.cache_dir.is_empty() {
            "/var/lib/cni/cache".to_string()
        } else {
            self.cache_dir.clone()
        };

        let path = Path::new(&cache_dir).join(netname);
        if !path.exists()
            && let Err(e) = std::fs::create_dir_all(&path)
        {
            warn!("Failed to create cache directory {}: {}", path.display(), e);
        }

        path
    }

    fn cache_network_config(
        &self,
        network_name: &str,
        rt: &RuntimeConf,
        config_bytes: &[u8],
    ) -> ResultCNI<()> {
        let cache_dir = self.get_cache_dir(network_name);
        let key = rt.get_cache_key();
        let config_path = cache_dir.join(format!("{key}.config"));

        debug!("Caching network config to {}", config_path.display());

        let mut file =
            fs::File::create(config_path).map_err(|e| Box::new(CNIError::Io(Box::new(e))))?;

        file.write_all(config_bytes)
            .map_err(|e| Box::new(CNIError::Io(Box::new(e))))?;

        Ok(())
    }

    fn cache_network_result(
        &self,
        network_name: &str,
        rt: &RuntimeConf,
        result: &dyn APIResult,
    ) -> ResultCNI<()> {
        let cache_dir = self.get_cache_dir(network_name);
        let key = rt.get_cache_key();
        let result_path = cache_dir.join(format!("{key}.result"));

        debug!("Caching network result to {}", result_path.display());

        let result_json = result.get_json();
        let result_bytes = result_json.dump().as_bytes().to_vec();

        let mut file =
            fs::File::create(result_path).map_err(|e| Box::new(CNIError::Io(Box::new(e))))?;

        file.write_all(&result_bytes)
            .map_err(|e| Box::new(CNIError::Io(Box::new(e))))?;

        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn read_cached_network(
        &self,
        netname: &str,
        rt: &RuntimeConf,
    ) -> Result<(Box<dyn APIResult>, Vec<u8>, RuntimeConf), String> {
        debug!("Reading cached network {netname} config");
        let cache_dir = self.get_cache_dir(netname);
        let key = rt.get_cache_key();
        let result_path = cache_dir.join(format!("{key}.result"));
        let config_path = cache_dir.join(format!("{key}.config"));

        if !result_path.exists() || !config_path.exists() {
            return Err("Cache files do not exist".to_string());
        }

        // Read files
        let result_bytes = fs::read(&result_path).map_err(|e| e.to_string())?;
        let config_bytes = fs::read(&config_path).map_err(|e| e.to_string())?;

        // Parse result
        let _: serde_json::Value = serde_json::from_slice(&result_bytes)
            .map_err(|e| format!("Failed to parse result cache: {e}"))?;

        // Create result object
        let result = result100::Result {
            cni_version: Some(
                rt.args
                    .iter()
                    .find(|arg| arg[0] == "cniVersion")
                    .map(|arg| arg[1].clone())
                    .unwrap_or_else(|| "0.3.1".to_string()),
            ),
            ..Default::default()
        };

        let result = Box::new(result) as Box<dyn APIResult>;

        Ok((result, config_bytes, rt.clone()))
    }

    fn build_new_config(
        &self,
        name: String,
        cni_version: String,
        orig: &NetworkConfig,
        prev_result: Option<Box<dyn APIResult>>,
        _rt: &RuntimeConf,
    ) -> Result<NetworkConfig, String> {
        debug!("Building new network config for {name}");

        let mut json_object = match json::parse(String::from_utf8_lossy(&orig.bytes).as_ref()) {
            Ok(obj) => obj,
            Err(e) => return Err(format!("Failed to parse network config: {e}")),
        };

        // Insert required fields
        if let Err(e) = json_object.insert("name", name) {
            return Err(format!("Failed to insert name: {e}"));
        }

        if let Err(e) = json_object.insert("cniVersion", cni_version) {
            return Err(format!("Failed to insert cniVersion: {e}"));
        }

        // Insert previous result (if provided)
        if let Some(prev_result) = prev_result {
            let prev_json = prev_result.get_json();
            debug!("Adding prevResult to config: {}", prev_json.dump());
            if let Err(e) = json_object.insert("prevResult", prev_json) {
                return Err(format!("Failed to insert prevResult: {e}"));
            }
        }

        let new_bytes = json_object.dump().as_bytes().to_vec();
        debug!("Built new config: {}", String::from_utf8_lossy(&new_bytes));

        // Create new config with updated bytes
        let mut new_conf = orig.clone();
        new_conf.bytes = new_bytes;

        Ok(new_conf)
    }
}

impl CNI for CNIConfig {
    fn add_network_list(
        &self,
        net: NetworkConfigList,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>> {
        info!("Adding network list: {}", net.name);

        // Validate the plugin chain
        self.validate_network_list(net.clone())?;

        let mut prev_result: Option<Box<dyn APIResult>> = None;

        // Apply each plugin in the chain
        for (i, plugin) in net.plugins.iter().enumerate() {
            debug!(
                "Executing plugin {}/{}: {}",
                i + 1,
                net.plugins.len(),
                plugin.network._type
            );

            // Add network with current plugin
            let result = self.add_network(
                net.name.clone(),
                net.cni_version.clone(),
                plugin.clone(),
                prev_result,
                rt.clone(),
            )?;

            // Update previous result for next plugin
            prev_result = Some(result);
        }

        // Cache the final result
        if let Some(result) = &prev_result
            && let Err(e) = self.cache_network_result(&net.name, &rt, result.as_ref())
        {
            warn!("Failed to cache network result: {e}");
        }

        debug!("Successfully added network list: {}", net.name);

        // Return the final result
        Ok(prev_result.unwrap_or_else(|| Box::<result100::Result>::default()))
    }

    fn check_network_list(&self, net: NetworkConfigList, rt: RuntimeConf) -> ResultCNI<()> {
        debug!("Checking network list: {}", net.name);

        // Skip check if disabled
        if net.disable_check {
            debug!("Network check is disabled for {}", net.name);
            return Ok(());
        }

        // Get cached result from previous add operation
        let (prev_result, _, _) = match self.read_cached_network(&net.name, &rt) {
            Ok(data) => data,
            Err(e) => {
                warn!("No cached result found for network {}: {}", net.name, e);
                (
                    Box::<result100::Result>::default() as Box<dyn APIResult>,
                    Vec::new(),
                    rt.clone(),
                )
            }
        };

        // Check each plugin in the chain
        for (i, plugin) in net.plugins.iter().enumerate() {
            debug!(
                "Checking plugin {}/{}: {}",
                i + 1,
                net.plugins.len(),
                plugin.network._type
            );

            // Check network with current plugin
            self.check_network(
                net.name.clone(),
                net.cni_version.clone(),
                Some(prev_result.clone_box()),
                plugin.clone(),
                rt.clone(),
            )?;
        }

        debug!("Network list check passed: {}", net.name);
        Ok(())
    }

    fn delete_network_list(&self, net: NetworkConfigList, rt: RuntimeConf) -> ResultCNI<()> {
        debug!("Deleting network list: {}", net.name);

        // Delete in reverse order
        for (i, plugin) in net.plugins.iter().enumerate().rev() {
            debug!(
                "Deleting plugin {}/{}: {}",
                net.plugins.len() - i,
                net.plugins.len(),
                plugin.network._type
            );

            // Delete network with current plugin
            if let Err(e) = self.delete_network(
                net.name.clone(),
                net.cni_version.clone(),
                plugin.clone(),
                rt.clone(),
            ) {
                error!("Error deleting plugin {}: {}", plugin.network._type, e);
                // Continue with next plugin even if one fails
            }
        }

        // Clean up cached data
        let cache_dir = self.get_cache_dir(&net.name);
        let key = rt.get_cache_key();
        let result_path = cache_dir.join(format!("{key}.result"));
        let config_path = cache_dir.join(format!("{key}.config"));

        if result_path.exists() {
            debug!("Removing cached result: {}", result_path.display());
            if let Err(e) = fs::remove_file(&result_path) {
                warn!("Failed to remove cached result: {e}");
            }
        }

        if config_path.exists() {
            debug!("Removing cached config: {}", config_path.display());
            if let Err(e) = fs::remove_file(&config_path) {
                warn!("Failed to remove cached config: {e}");
            }
        }

        debug!("Successfully deleted network list: {}", net.name);
        Ok(())
    }

    fn get_network_list_cached_result(
        &self,
        net: NetworkConfigList,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>> {
        debug!("Getting cached result for network list: {}", net.name);

        match self.read_cached_network(&net.name, &rt) {
            Ok((result, _, _)) => {
                debug!("Found cached result for network {}", net.name);
                Ok(result)
            }
            Err(e) => {
                let err_msg = format!("No cached result for network {}: {}", net.name, e);
                error!("{err_msg}");
                Err(Box::new(CNIError::NotFound(net.name, err_msg)))
            }
        }
    }

    fn add_network(
        &self,
        name: String,
        cni_version: String,
        net: NetworkConfig,
        prev_result: Option<Box<dyn APIResult>>,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>> {
        debug!("Adding network {} with plugin {name}", net.network._type);

        // Find plugin path
        let plugin_path = self
            .exec
            .find_in_path(net.network._type.clone(), self.path.clone())?;

        // Setup environment
        let environ = ExecArgs {
            command: "ADD".to_string(),
            containerd_id: rt.container_id.clone(),
            netns: rt.net_ns.clone(),
            plugin_args: rt.args.clone(),
            plugin_args_str: rt
                .args
                .iter()
                .map(|arg| format!("{}={}", arg[0], arg[1]))
                .collect::<Vec<_>>()
                .join(";"),
            ifname: rt.if_name.clone(),
            path: self.path[0].clone(),
        };

        // Build new config with name, version and prevResult
        let new_conf = match self.build_new_config(
            name.clone(),
            cni_version.clone(),
            &net,
            prev_result,
            &rt,
        ) {
            Ok(conf) => conf,
            Err(e) => return Err(Box::new(CNIError::Config(e))),
        };

        // Cache network config
        if let Err(e) = self.cache_network_config(&name, &rt, &new_conf.bytes) {
            warn!("Failed to cache network config: {e}");
        }

        // Execute plugin
        let result_bytes =
            self.exec
                .exec_plugins(plugin_path, &new_conf.bytes, environ.to_env())?;

        // Directly deserialize the result JSON into the result structure
        let mut result: result100::Result = match serde_json::from_slice(&result_bytes) {
            Ok(r) => r,
            Err(e) => {
                // If direct deserialization fails, create a default result with minimal information
                debug!("Failed to directly deserialize result: {e}, creating minimal result");
                result100::Result {
                    cni_version: Some(cni_version.clone()),
                    ..Default::default()
                }
            }
        };

        // Ensure CNI version is set
        if result.cni_version.is_none() {
            result.cni_version = Some(cni_version);
        }

        debug!("Successfully added network {name}");
        Ok(Box::new(result))
    }

    fn check_network(
        &self,
        name: String,
        cni_version: String,
        prev_result: Option<Box<dyn APIResult>>,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<()> {
        debug!(
            "Checking network {} with plugin {}",
            name, net.network._type
        );

        // Find plugin in path
        let plugin_path = self
            .exec
            .find_in_path(net.network._type.clone(), self.path.clone())?;

        // Set up environment
        let environ = ExecArgs {
            command: "CHECK".to_string(),
            containerd_id: rt.container_id.clone(),
            netns: rt.net_ns.clone(),
            plugin_args: rt.args.clone(),
            plugin_args_str: rt
                .args
                .iter()
                .map(|arg| format!("{}={}", arg[0], arg[1]))
                .collect::<Vec<_>>()
                .join(";"),
            ifname: rt.if_name.clone(),
            path: self.path[0].clone(),
        };

        // Build new config with name, version and prevResult
        let new_conf =
            match self.build_new_config(name.clone(), cni_version, &net, prev_result, &rt) {
                Ok(conf) => conf,
                Err(e) => return Err(Box::new(CNIError::Config(e))),
            };

        // Execute plugin
        self.exec
            .exec_plugins(plugin_path, &new_conf.bytes, environ.to_env())?;

        debug!("Network check passed for {name}");
        Ok(())
    }

    fn delete_network(
        &self,
        name: String,
        cni_version: String,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<()> {
        debug!(
            "Deleting network {} with plugin {}",
            name, net.network._type
        );

        // Find plugin in path
        let plugin_path = self
            .exec
            .find_in_path(net.network._type.clone(), self.path.clone())?;

        // Set up environment
        let environ = ExecArgs {
            command: "DEL".to_string(),
            containerd_id: rt.container_id.clone(),
            netns: rt.net_ns.clone(),
            plugin_args: rt.args.clone(),
            plugin_args_str: rt
                .args
                .iter()
                .map(|arg| format!("{}={}", arg[0], arg[1]))
                .collect::<Vec<_>>()
                .join(";"),
            ifname: rt.if_name.clone(),
            path: self.path[0].clone(),
        };

        // Build new config with name and version
        let new_conf = match self.build_new_config(name.clone(), cni_version, &net, None, &rt) {
            Ok(conf) => conf,
            Err(e) => return Err(Box::new(CNIError::Config(e))),
        };

        // Execute plugin
        self.exec
            .exec_plugins(plugin_path, &new_conf.bytes, environ.to_env())?;

        debug!("Successfully deleted network {name}");
        Ok(())
    }

    fn get_network_cached_result(
        &self,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<Box<dyn APIResult>> {
        debug!("Getting cached result for network {}", net.network.name);

        match self.read_cached_network(&net.network.name, &rt) {
            Ok((result, _, _)) => {
                debug!("Found cached result for network {}", net.network.name);
                Ok(result)
            }
            Err(e) => {
                let err_msg = format!("No cached result for network {}: {}", net.network.name, e);
                error!("{err_msg}");
                Err(Box::new(CNIError::NotFound(net.network.name, err_msg)))
            }
        }
    }

    fn get_network_cached_config(
        &self,
        net: NetworkConfig,
        rt: RuntimeConf,
    ) -> ResultCNI<(Vec<u8>, RuntimeConf)> {
        debug!("Getting cached config for network {}", net.network.name);

        match self.read_cached_network(&net.network.name, &rt) {
            Ok((_, config_bytes, cached_rt)) => {
                debug!("Found cached config for network {}", net.network.name);
                Ok((config_bytes, cached_rt))
            }
            Err(e) => {
                let err_msg = format!("No cached config for network {}: {}", net.network.name, e);
                error!("{err_msg}");
                Err(Box::new(CNIError::NotFound(net.network.name, err_msg)))
            }
        }
    }

    fn validate_network_list(&self, net: NetworkConfigList) -> ResultCNI<Vec<String>> {
        debug!("Validating network list: {}", net.name);

        // Check basic requirements
        if let Err(e) = net.validate() {
            return Err(Box::new(CNIError::Config(e)));
        }

        // Validate each plugin
        let mut plugin_types = Vec::new();
        for plugin in &net.plugins {
            let types = self.validate_network(plugin.clone())?;
            plugin_types.extend(types);
        }

        debug!("Network list validation passed for {}", net.name);
        Ok(plugin_types)
    }

    fn validate_network(&self, net: NetworkConfig) -> ResultCNI<Vec<String>> {
        debug!("Validating network: {}", net.network.name);

        // Check basic requirements
        if net.network._type.is_empty() {
            return Err(Box::new(CNIError::Config(
                "Plugin type cannot be empty".to_string(),
            )));
        }

        // Find plugin in path
        let plugin_path = self
            .exec
            .find_in_path(net.network._type.clone(), self.path.clone())?;

        // Set up environment for VERSION command
        let environ = ExecArgs {
            command: "VERSION".to_string(),
            containerd_id: "".to_string(),
            netns: "".to_string(),
            plugin_args: Vec::new(),
            plugin_args_str: "".to_string(),
            ifname: "".to_string(),
            path: self.path[0].clone(),
        };

        // Execute plugin with VERSION command
        match self.exec.exec_plugins(plugin_path, &[], environ.to_env()) {
            Ok(version_bytes) => {
                // Parse version info
                match serde_json::from_slice::<serde_json::Value>(&version_bytes) {
                    Ok(version_info) => {
                        if let Some(supported_versions) = version_info.get("supportedVersions")
                            && let Some(versions_array) = supported_versions.as_array()
                        {
                            let versions: Vec<String> = versions_array
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();

                            debug!(
                                "Plugin {} supports versions: {:?}",
                                net.network._type, versions
                            );
                            return Ok(versions);
                        }

                        warn!(
                            "Plugin {} did not return supported versions",
                            net.network._type
                        );
                        Ok(vec![])
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse version info from plugin {}: {}",
                            net.network._type, e
                        );
                        Ok(vec![])
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to get version info from plugin {}: {}",
                    net.network._type, e
                );
                Ok(vec![])
            }
        }
    }
}
