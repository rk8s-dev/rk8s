use std::collections::HashMap;

use crate::commands::compose::spec::ComposeSpec;
use crate::commands::compose::spec::NetworkSpec;
use crate::commands::compose::spec::ServiceSpec;
use anyhow::Ok;
use anyhow::Result;
use anyhow::anyhow;

pub struct NetworkManager {
    map: HashMap<String, NetworkSpec>,
    /// key: service_name value: networks
    service_mapping: HashMap<String, Vec<String>>,
    /// key: network_name value: (srv_name, service_spec) k
    network_service: HashMap<String, Vec<(String, ServiceSpec)>>,
    /// if there is no network definition then just create a default network
    is_default: bool,
    project_name: String,
}

impl NetworkManager {
    pub fn new(project_name: String) -> Self {
        Self {
            map: HashMap::new(),
            service_mapping: HashMap::new(),
            is_default: false,
            network_service: HashMap::new(),
            project_name,
        }
    }

    pub fn network_service_mapping(&self) -> HashMap<String, Vec<(String, ServiceSpec)>> {
        self.network_service.clone()
    }

    pub fn handle(&mut self, spec: &ComposeSpec) -> Result<()> {
        // read the networks
        if let Some(networks_spec) = &spec.networks {
            self.map = networks_spec.0.clone()
        } else {
            // there is no definition of networks
            self.is_default = true
        }

        self.validate(spec)?;

        Ok(())
    }

    pub fn is_default(&self) -> bool {
        self.is_default
    }

    pub fn write_network_config(&self) -> Result<()> {
        Ok(())
    }

    /// validate the correctness and initialize  the service_mapping
    fn validate(&mut self, spec: &ComposeSpec) -> Result<()> {
        for (srv, srv_spec) in &spec.services {
            // if the srv does not have the network definition then add to the default network
            if srv_spec.networks.is_empty() {
                self.network_service
                    .entry(format!("{}_default", self.project_name))
                    .or_default()
                    .push((srv.clone(), srv_spec.clone()));
            }
            for network_name in &srv_spec.networks {
                if !self.map.contains_key(network_name) {
                    return Err(anyhow!(
                        "wrong networks network {} is not defined",
                        network_name
                    ));
                }
                self.service_mapping
                    .entry(srv.clone())
                    .or_default()
                    .push(network_name.clone());

                self.network_service
                    .entry(network_name.clone())
                    .or_default()
                    .push((srv.clone(), srv_spec.clone()));
            }
        }
        // all service doesn't have the network definition create a default network
        if self.is_default {
            let network_name = format!("{}_default", self.project_name);

            let services: Vec<(String, ServiceSpec)> = spec
                .services
                .iter()
                .map(|(name, spec)| (name.clone(), spec.clone()))
                .collect();

            self.network_service.insert(network_name, services);
        }
        Ok(())
    }

    // fn setup_network(&self) -> Result<()> {
    //     Ok(())
    // }
}
