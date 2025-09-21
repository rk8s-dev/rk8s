// Copyright (c) 2024 https://github.com/jokemanfire/rust-cni
use log::{debug, error, trace};
use std::{collections::HashMap, sync::Arc};

use crate::rust_cni::{
    self,
    api::{CNI, RuntimeConf},
    result::{APIResult, ResultCNI},
};

pub struct Network {
    pub cni: Arc<Box<dyn CNI + Send + Sync>>,
    pub config: rust_cni::api::NetworkConfigList,
    pub ifname: String,
}

impl Network {
    pub fn attach(&self, ns: &Namespace) -> ResultCNI<Box<dyn APIResult>> {
        debug!(
            "Attaching network {} with interface {}",
            self.config.name, self.ifname
        );

        match self
            .cni
            .add_network_list(self.config.clone(), ns.config(self.ifname.clone()))
        {
            Ok(result) => {
                debug!(
                    "Successfully attached network {} to namespace",
                    self.config.name
                );
                trace!("Network attachment result: {:?}", result.get_json());
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    pub fn remove(&self, ns: &Namespace) -> Result<(), String> {
        debug!(
            "Removing network {} with interface {}",
            self.config.name, self.ifname
        );

        match self
            .cni
            .delete_network_list(self.config.clone(), ns.config(self.ifname.clone()))
        {
            Ok(_) => {
                debug!(
                    "Successfully removed network {} from namespace",
                    self.config.name
                );
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to remove network {}: {}", self.config.name, e);
                error!("{err_msg}");
                Err(err_msg)
            }
        }
    }

    pub fn check(&self, ns: &Namespace) -> Result<(), String> {
        debug!(
            "Checking network {} with interface {}",
            self.config.name, self.ifname
        );

        match self
            .cni
            .check_network_list(self.config.clone(), ns.config(self.ifname.clone()))
        {
            Ok(_) => {
                debug!("Network {} is properly configured", self.config.name);
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Network check failed for {}: {}", self.config.name, e);
                error!("{err_msg}");
                Err(err_msg)
            }
        }
    }

    pub fn get_stats(&self, ns: &Namespace) -> Result<String, String> {
        debug!(
            "Getting stats for network {} with interface {}",
            self.config.name, self.ifname
        );

        match self
            .cni
            .get_network_list_cached_result(self.config.clone(), ns.config(self.ifname.clone()))
        {
            Ok(result) => {
                let stats_json = result.get_json().dump();
                debug!("Network stats: {stats_json}");
                Ok(stats_json)
            }
            Err(e) => {
                let err_msg = format!(
                    "Failed to get stats for network {}: {}",
                    self.config.name, e
                );
                error!("{err_msg}");
                Err(err_msg)
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct Namespace {
    id: String,
    path: String,
    capability_args: HashMap<String, String>,
    args: HashMap<String, String>,
}

impl Namespace {
    pub fn new(id: String, path: String) -> Self {
        debug!("Creating new namespace for container {id} at path {path}");
        Self {
            id,
            path,
            capability_args: HashMap::default(),
            args: HashMap::default(),
        }
    }

    pub fn with_args(mut self, args: HashMap<String, String>) -> Self {
        debug!("Adding {} arguments to namespace", args.len());
        self.args = args;
        self
    }

    pub fn with_capabilities(mut self, capabilities: HashMap<String, String>) -> Self {
        debug!("Adding {} capabilities to namespace", capabilities.len());
        self.capability_args = capabilities;
        self
    }

    pub fn add_arg(&mut self, key: &str, value: &str) {
        debug!("Adding argument {key}={value} to namespace");
        self.args.insert(key.to_string(), value.to_string());
    }

    pub fn add_capability(&mut self, key: &str, value: &str) {
        debug!("Adding capability {key}={value} to namespace");
        self.capability_args
            .insert(key.to_string(), value.to_string());
    }

    pub fn config(&self, ifname: String) -> rust_cni::api::RuntimeConf {
        debug!("Creating runtime config for namespace with interface {ifname}");
        let args = self
            .args
            .iter()
            .map(|(key, val)| [key.clone(), val.clone()])
            .collect();

        RuntimeConf {
            container_id: self.id.clone(),
            net_ns: self.path.clone(),
            if_name: ifname,
            args,
            capability_args: self.capability_args.clone(),
            cache_dir: String::default(),
        }
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }
}
