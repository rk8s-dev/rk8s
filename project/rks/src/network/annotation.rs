use anyhow::{anyhow, Result};
use regex::Regex;

/// Kubernetes annotations
#[derive(Debug, Clone)]
pub struct Annotations {
    pub subnet_kube_managed: String,
    pub backend_data: String,
    pub backend_v6_data: String,
    pub backend_type: String,
    pub backend_public_ip: String,
    pub backend_node_public_ip: String,
    pub backend_public_ip_overwrite: String,
    pub backend_public_ipv6: String,
    pub backend_node_public_ipv6: String,
    pub backend_public_ipv6_overwrite: String,
}

///  prefix 的 Kubernetes annotations
pub fn new_annotations(prefix: &str) -> Result<Annotations> {
    let mut prefix = prefix.to_string();
    let slash_count = prefix.matches('/').count();

    if slash_count > 1 {
        return Err(anyhow!(
            "subnet/kube: prefix can contain at most single slash"
        ));
    }

    if slash_count == 0 {
        prefix.push('/');
    }

    if !prefix.ends_with('/') && !prefix.ends_with('-') {
        prefix.push('-');
    }

    let re = Regex::new(r"^(?:[a-z0-9_-]+\.)+[a-z0-9_-]+/(?:[a-z0-9_-]+-)?$")?;
    if !re.is_match(&prefix) {
        return Err(anyhow!(
            "subnet/kube: prefix must be in a format: fqdn/[0-9a-z-_]*"
        ));
    }

    Ok(Annotations {
        subnet_kube_managed: format!("{}rks-subnet-manager", prefix),
        backend_data: format!("{}backend-data", prefix),
        backend_v6_data: format!("{}backend-v6-data", prefix),
        backend_type: format!("{}backend-type", prefix),
        backend_public_ip: format!("{}public-ip", prefix),
        backend_node_public_ip: format!("{}node-public-ip", prefix),
        backend_public_ip_overwrite: format!("{}public-ip-overwrite", prefix),
        backend_public_ipv6: format!("{}public-ipv6", prefix),
        backend_node_public_ipv6: format!("{}node-public-ipv6", prefix),
        backend_public_ipv6_overwrite: format!("{}public-ipv6-overwrite", prefix),
    })
}

