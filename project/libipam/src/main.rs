use std::sync::Arc;

use allocator::IpAllocator;
use anyhow::bail;
use cni_plugin::{
    Cni, Command, Inputs,
    config::NetworkConfig,
    error::CniError,
    reply::{IpamSuccessReply, reply},
};
use config::{IPAMConfig, Net};
use disk::Store;
use log::{debug, error, info};
use range_set::RangeSetExt;

mod allocator;
mod config;
mod disk;
mod range;
mod range_set;

fn main() {
    //cni_plugin::logger::install(env!("CARGO_PKG_NAME"));
    debug!(
        "{} (CNI IPAM plugin) version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let inputs: Inputs = Cni::load().into_inputs().unwrap();
    let cni_version = inputs.config.cni_version.clone();

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
            Command::Add => cmd_add(inputs).await,
            Command::Del => cmd_del(inputs).await,
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

async fn cmd_add(inputs: Inputs) -> Result<IpamSuccessReply, CniError> {
    let config = match load_ipam_netconf(&inputs.config) {
        Ok(conf) => conf,
        Err(err) => {
            return Err(CniError::Generic(err.to_string()));
        }
    };

    let store =
        Arc::new(Store::new(config.data_dir).map_err(|e| CniError::Generic(e.to_string()))?);
    let mut allocators: Vec<IpAllocator> = vec![];
    let mut exec_result = IpamSuccessReply {
        cni_version: inputs.config.cni_version,
        ips: Default::default(),
        routes: Default::default(),
        dns: Default::default(),
        specific: Default::default(),
    };

    let mut ips = vec![];
    for (idx, rangeset) in config.ranges.into_iter().enumerate() {
        let allocator = IpAllocator::new(rangeset, store.clone(), idx);
        let result = allocator.get(&inputs.container_id, &inputs.ifname, None);
        match result {
            Ok(ip) => {
                ips.push(ip);
            }
            Err(e) => {
                for alloc in &allocators {
                    let _ = alloc.release(&inputs.container_id, &inputs.ifname);
                }
                return Err(CniError::Generic(e.to_string()));
            }
        }
        allocators.push(allocator);
    }

    exec_result.ips = ips;
    exec_result.routes = config.routes.unwrap_or_default();
    Ok(exec_result)
}

async fn cmd_del(inputs: Inputs) -> Result<IpamSuccessReply, CniError> {
    let config = match load_ipam_netconf(&inputs.config) {
        Ok(conf) => conf,
        Err(err) => {
            return Err(CniError::Generic(err.to_string()));
        }
    };

    let store =
        Arc::new(Store::new(config.data_dir).map_err(|e| CniError::Generic(e.to_string()))?);

    let exec_result = IpamSuccessReply {
        cni_version: inputs.config.cni_version,
        ips: Default::default(),
        routes: Default::default(),
        dns: Default::default(),
        specific: Default::default(),
    };

    let mut errors = Vec::new();
    for (idx, rangeset) in config.ranges.into_iter().enumerate() {
        let allocator = IpAllocator::new(rangeset, store.clone(), idx);
        if let Err(e) = allocator.release(&inputs.container_id, &inputs.ifname) {
            errors.push(e.to_string());
        }
    }

    if !errors.is_empty() {
        let combined_error = errors.join("; ");
        return Err(CniError::Generic(combined_error));
    }

    Ok(exec_result)
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
