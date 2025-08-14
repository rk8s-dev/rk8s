use anyhow::{Result, anyhow};
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
        subnet_kube_managed: format!("{prefix}rks-subnet-manager"),
        backend_data: format!("{prefix}backend-data"),
        backend_v6_data: format!("{prefix}backend-v6-data"),
        backend_type: format!("{prefix}backend-type"),
        backend_public_ip: format!("{prefix}public-ip"),
        backend_node_public_ip: format!("{prefix}node-public-ip"),
        backend_public_ip_overwrite: format!("{prefix}public-ip-overwrite"),
        backend_public_ipv6: format!("{prefix}public-ipv6"),
        backend_node_public_ipv6: format!("{prefix}node-public-ipv6"),
        backend_public_ipv6_overwrite: format!("{prefix}public-ipv6-overwrite"),
    })
}
