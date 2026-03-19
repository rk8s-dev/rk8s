use anyhow::bail;
use cni_plugin::{
    Cni, Command, Inputs,
    config::NetworkConfig,
    error::CniError,
    reply::{IpamSuccessReply, Route, reply},
};
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::ipam::allocator::IpAllocator;
use crate::ipam::config::{IPAMConfig, Net};
use crate::ipam::disk::Store;
use crate::ipam::range_set::RangeSetExt;

pub fn run() {
    //cni_plugin::logger::install(env!("CARGO_PKG_NAME"));
    debug!(
        "{} (CNI IPAM plugin) version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let inputs: Inputs = Cni::load().into_inputs().unwrap();
    let cni_version = inputs.config.cni_version.clone();
    let net_conf = inputs.config.clone();
    let contianer_id = inputs.container_id.clone();
    let if_name = inputs.ifname.clone();

    info!(
        "{} serving spec v{} for command={:?}",
        env!("CARGO_PKG_NAME"),
        cni_version,
        inputs.command
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let res: Result<IpamSuccessReply, CniError> = rt.block_on(async move {
        match inputs.command {
            Command::Add => cmd_add(&net_conf, &contianer_id, &if_name).await,
            Command::Del => cmd_del(&net_conf, &contianer_id, &if_name).await,
            Command::Check => todo!(),
            Command::Version => unreachable!(),
        }
    });

    match res {
        Ok(res) => {
            debug!("success! {res:#?}");
            reply(res)
        }
        Err(res) => {
            error!("error: {res}");
            reply(res.into_reply(cni_version))
        }
    }
}

/// Execute IP allocation for one CNI ADD operation.
pub async fn cmd_add(
    net_conf: &NetworkConfig,
    container_id: &str,
    ifname: &str,
) -> Result<IpamSuccessReply, CniError> {
    let config = load_ipam_netconf(net_conf).map_err(|e| CniError::Generic(e.to_string()))?;

    let store = Arc::new(
        Store::new(config.data_dir.clone()).map_err(|e| CniError::Generic(e.to_string()))?,
    );

    let mut allocators: Vec<IpAllocator> = Vec::new();
    let mut ips = Vec::new();
    let cni_version = net_conf.cni_version.clone();
    let routes: Vec<Route> = config.routes.clone().unwrap_or_default();

    for (idx, rangeset) in config.ranges.into_iter().enumerate() {
        let allocator = IpAllocator::new(rangeset, store.clone(), idx);
        match allocator.get(container_id, ifname, None) {
            Ok(ip) => ips.push(ip),
            Err(e) => {
                for alloc in &allocators {
                    let _ = alloc.release(container_id, ifname);
                }
                return Err(CniError::Generic(e.to_string()));
            }
        }
        allocators.push(allocator);
    }

    Ok(IpamSuccessReply {
        cni_version,
        ips,
        routes,
        dns: Default::default(),
        specific: Default::default(),
    })
}

/// Execute IP release for one CNI DEL operation.
pub async fn cmd_del(
    net_conf: &NetworkConfig,
    container_id: &str,
    ifname: &str,
) -> Result<IpamSuccessReply, CniError> {
    let config = load_ipam_netconf(net_conf).map_err(|e| CniError::Generic(e.to_string()))?;

    let store = Arc::new(
        Store::new(config.data_dir.clone()).map_err(|e| CniError::Generic(e.to_string()))?,
    );

    let mut errors = Vec::new();
    for (idx, rangeset) in config.ranges.into_iter().enumerate() {
        let allocator = IpAllocator::new(rangeset, store.clone(), idx);
        if let Err(e) = allocator.release(container_id, ifname) {
            errors.push(e.to_string());
        }
    }

    if !errors.is_empty() {
        return Err(CniError::Generic(errors.join("; ")));
    }

    Ok(IpamSuccessReply {
        cni_version: net_conf.cni_version.clone(),
        ips: Default::default(),
        routes: Default::default(),
        dns: Default::default(),
        specific: Default::default(),
    })
}

pub fn load_ipam_netconf(net_conf: &NetworkConfig) -> anyhow::Result<IPAMConfig> {
    let json_value = serde_json::to_value(net_conf)?;
    let mut net: Net = serde_json::from_value(json_value)?;

    if net.ipam.ranges.is_empty() {
        bail!("no IP ranges specified")
    }

    for entry in net.ipam.ranges.iter_mut() {
        entry.canonicalize()?;
    }

    let l = net.ipam.ranges.len();
    for i in 0..l {
        for j in i + 1..l {
            if net.ipam.ranges[i].overlap(&net.ipam.ranges[j]) {
                bail!("range set {} overlaps with {}", i, i + j + 1)
            }
        }
    }

    net.ipam.name = Some(net.name.clone());
    Ok(net.ipam)
}
