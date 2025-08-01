use std::net::{IpAddr, Ipv4Addr};

use crate::ip::addr;
use crate::ip::link;
use crate::ip::utils;

use anyhow::anyhow;
use anyhow::bail;
use cni_plugin::reply::SuccessReply;
use log::debug;

pub async fn config_interface(if_name: &str, exec_result: &SuccessReply) -> anyhow::Result<()> {
    let link = link::link_by_name(if_name)
        .await
        .map_err(|e| anyhow!("{}", e))?;

    let ips = &exec_result.ips;
    if exec_result.ips.is_empty() {
        return Err(anyhow!("ips not found"));
    }
    debug!("ips:{ips:?}");
    for ip in ips {
        if ip.interface.is_none() {
            continue;
        }
        let int_idx = ip.interface.unwrap();
        if int_idx >= exec_result.interfaces.len()
            || exec_result.interfaces[int_idx].name != if_name
        {
            bail!(
                "failed to add IP addr to {}: invalid interface index",
                if_name
            );
        }
        // add address to veth interface
        debug!("add ip:{ips:?} to:{if_name:?}");
        addr::addr_add(link.header.index, ip.address.ip(), ip.address.prefix()).await?;
    }

    link::link_set_up(&link).await?;

    let routes = &exec_result.routes;
    if !routes.is_empty() {
        for route in routes {
            link::route_add(route.clone()).await?;
        }
    }

    Ok(())
}

pub fn next_ip(ip: &IpAddr) -> Option<IpAddr> {
    match ip {
        IpAddr::V4(ipv4) => {
            let ip = ipv4.octets();
            let ip_num = u32::from_be_bytes(ip);
            let (ip_num, overflow) = ip_num.overflowing_add(1);
            if overflow {
                return None;
            }
            Some(IpAddr::V4(Ipv4Addr::from(ip_num.to_be_bytes())))
        }
        IpAddr::V6(_ipv6) => None,
    }
}

pub fn enable_ipv4_forward() -> anyhow::Result<()> {
    utils::sysctl_set("net/ipv4/ip_forward", "1")
}

pub fn enable_ipv6_forward() -> anyhow::Result<()> {
    utils::sysctl_set("net/ipv6/conf/all/forwarding", "1")
}
