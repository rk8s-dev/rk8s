// Copyright (c) 2024 https://github.com/divinerapier/cni-rs
use std::io::stdout;

use json::JsonValue;
use serde::{Deserialize, Serialize};
use serde_json::to_string;

use crate::rust_cni::error::CNIError;

use super::APIResult;

// const IMPLEMENTED_SPEC_VERSION: &'static str = "1.0.0";

#[derive(Serialize, Deserialize, Clone)]
pub struct Interface {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IPConfig {
    #[serde(rename = "interface")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<usize>,
    #[serde(rename = "address")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<ipnetwork::IpNetwork>,
    #[serde(rename = "gateway")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<std::net::IpAddr>,
}

#[derive(Serialize, Deserialize, Default, Clone)]

pub struct Result {
    #[serde(rename = "cniVersion")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cni_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interfaces: Option<Vec<Interface>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ips: Option<Vec<IPConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<super::super::types::Route>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<super::super::types::DNS>,
}

#[typetag::serde]
impl APIResult for Result {
    fn version(&self) -> String {
        if let Some(cni_version) = &self.cni_version {
            return cni_version.clone();
        }
        String::default()
    }

    fn get_as_version(&self, _version: String) -> super::ResultCNI<Box<dyn APIResult>> {
        Ok(Box::<Result>::default())
    }

    fn print(&self) -> super::ResultCNI<()> {
        self.print_to(Box::new(stdout()))
    }

    fn print_to(&self, mut w: Box<dyn std::io::Write>) -> super::ResultCNI<()> {
        let json_data = to_string(&self).unwrap();
        w.write(json_data.as_bytes())
            .map_err(|e| CNIError::Io(Box::new(e)))?;
        Ok(())
    }

    fn get_json(&self) -> JsonValue {
        let js_string = to_string(&self).unwrap();
        json::parse(&js_string).unwrap()
    }

    fn clone_box(&self) -> Box<dyn APIResult> {
        let cloned = Result {
            cni_version: self.cni_version.clone(),
            interfaces: self.interfaces.clone(),
            ips: self.ips.clone(),
            routes: self.routes.clone(),
            dns: self.dns.clone(),
        };
        Box::new(cloned)
    }
}
