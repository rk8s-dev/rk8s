#![allow(dead_code)]
use std::net::Ipv4Addr;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone, Debug)]
pub struct ServiceRecord {
    pub name: String,
    pub namespace: String,
    pub cluster_ip: Option<Ipv4Addr>,
}

#[derive(Clone, Debug)]
pub struct PodRecord {
    pub name: String,
    pub namespace: String,
    pub pod_ip: Option<Ipv4Addr>,
}

#[derive(Debug)]
pub struct DnsObjectCache {
    pub service_cache: Arc<RwLock<HashMap<(String, String), ServiceRecord>>>, // key: (ns, name)
    pub pod_cache: Arc<RwLock<HashMap<(String, String), PodRecord>>>,
}

impl DnsObjectCache {
    pub fn new() -> Self {
        Self {
            service_cache: Arc::new(RwLock::new(HashMap::new())),
            pod_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DnsObjectCache {
    fn default() -> Self {
        Self::new()
    }
}
