use std::collections;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeSpec {
    #[serde(default)]
    pub services: collections::HashMap<String, ServiceSpec>,

    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,

    #[serde(default)]
    pub configs: Vec<ConfigSpec>,

    #[serde(default)]
    pub networks: Vec<NetworkSpec>,

    #[serde(default)]
    pub secrets: Vec<SecretSpec>,

    #[serde(default)]
    pub command: Vec<String>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceSpec {
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,

    #[serde(default)]
    pub configs: Option<Vec<String>>,

    #[serde(default)]
    pub secrets: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretSpec {}

pub fn run_compose(compose_yaml: Option<String>) -> Result<(), anyhow::Error> {
    println!("{:?}", compose_yaml.unwrap_or_default());
    Ok(())
}
