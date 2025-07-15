use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeSpec {
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub services: HashMap<String, ServiceSpec>,

    #[serde(default)]
    pub volumes: Option<VolumeSpec>,

    #[serde(default)]
    pub configs: Vec<ConfigSpec>,

    #[serde(default)]
    pub networks: Vec<NetworkSpec>,

    #[serde(default)]
    pub secrets: Vec<SecretSpec>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceSpec {
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub networks: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub command: Vec<String>,

    #[serde(default)]
    pub configs: Option<Vec<String>>,

    #[serde(default)]
    pub secrets: Option<Vec<String>>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeSpec(HashMap<String, Vec<String>>);

impl VolumeSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretSpec {}
