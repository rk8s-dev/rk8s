use anyhow::{Result, anyhow};
use libcni::ip::route::interfaces;
use log::{info, warn};
use std::net::Ipv4Addr;

use super::ExternalInterface;
use libnetwork::iface::{
    get_default_gateway_interface, get_interface_by_ipv4, get_interface_ipv4_addrs,
    get_interface_ipv6_addrs,
};

/// Discover and validate external network interface
pub async fn discover_interface(
    iface_name: Option<&str>,
    expected_ip: Option<Ipv4Addr>,
) -> Result<ExternalInterface> {
    info!("Discovering external network interface");

    // For demonstration purposes, we'll create a mock interface
    // In a real implementation, this would use system calls to discover network interfaces
    let interface = if let Some(name) = iface_name {
        discover_interface_by_name(name).await?
    } else if let Some(ip) = expected_ip {
        discover_interface_by_ip(ip).await?
    } else {
        discover_default_interface().await?
    };

    validate_interface(&interface)?;

    info!(
        "Discovered interface: {} (index: {}, MTU: {}, addr: {:?})",
        interface.iface.name,
        interface.iface.index,
        interface.iface.mtu.unwrap_or(1500),
        interface.iface_addr
    );

    Ok(interface)
}

async fn discover_interface_by_name(name: &str) -> Result<ExternalInterface> {
    info!("Looking for interface by name: {name}");

    let all_interfaces = interfaces().await?;

    let iface = all_interfaces
        .into_iter()
        .find(|iface| iface.name == name)
        .ok_or_else(|| anyhow!("Interface '{name}' not found"))?;

    let ipv4_addrs = get_interface_ipv4_addrs(iface.index)
        .await
        .unwrap_or_default();
    let ipv6_addrs = get_interface_ipv6_addrs(iface.index)
        .await
        .unwrap_or_default();

    let iface_addr = ipv4_addrs.first().copied();
    let iface_v6_addr = ipv6_addrs.first().copied();

    let interface = ExternalInterface {
        iface,
        iface_addr,
        ext_addr: iface_addr,
        iface_v6_addr,
        ext_v6_addr: iface_v6_addr,
    };

    Ok(interface)
}

async fn discover_interface_by_ip(ip: Ipv4Addr) -> Result<ExternalInterface> {
    info!("Looking for interface by IP: {ip}");

    let iface = get_interface_by_ipv4(ip).await?;

    let ipv4_addrs = get_interface_ipv4_addrs(iface.index)
        .await
        .unwrap_or_default();
    let ipv6_addrs = get_interface_ipv6_addrs(iface.index)
        .await
        .unwrap_or_default();

    let iface_addr = ipv4_addrs.iter().find(|&&addr| addr == ip).copied();
    let iface_v6_addr = ipv6_addrs.first().copied();

    let interface = ExternalInterface {
        iface,
        iface_addr,
        ext_addr: iface_addr,
        iface_v6_addr,
        ext_v6_addr: iface_v6_addr,
    };

    Ok(interface)
}

async fn discover_default_interface() -> Result<ExternalInterface> {
    info!("Discovering default network interface");

    let iface = get_default_gateway_interface().await?;

    let ipv4_addrs = get_interface_ipv4_addrs(iface.index)
        .await
        .unwrap_or_default();
    let ipv6_addrs = get_interface_ipv6_addrs(iface.index)
        .await
        .unwrap_or_default();

    let iface_addr = ipv4_addrs.first().copied();
    let iface_v6_addr = ipv6_addrs.first().copied();

    let interface = ExternalInterface {
        iface,
        iface_addr,
        ext_addr: iface_addr,
        iface_v6_addr,
        ext_v6_addr: iface_v6_addr,
    };

    Ok(interface)
}

fn validate_interface(interface: &ExternalInterface) -> Result<()> {
    // Validate that the interface is suitable for host-gw backend
    let mtu = interface.iface.mtu.unwrap_or(1500);
    if mtu < 1280 {
        return Err(anyhow!("Interface MTU {mtu} is too small, minimum 1280"));
    }

    let iface_addr = interface
        .iface_addr
        .ok_or_else(|| anyhow!("Interface has no IPv4 address assigned"))?;

    if iface_addr.is_unspecified() {
        return Err(anyhow!("Interface has invalid IPv4 address"));
    }

    // Check if we're behind NAT (PublicIP differs from interface IP)
    if let Some(ext_addr) = interface.ext_addr
        && ext_addr != iface_addr
    {
        warn!(
            "External IP ({ext_addr}) differs from interface IP ({iface_addr}), this may indicate NAT which is not supported by host-gw backend"
        );
    }

    info!("Interface validation passed");
    Ok(())
}

/// Check if the interface configuration is compatible with host-gw backend
pub fn check_hostgw_compatibility(interface: &ExternalInterface) -> Result<()> {
    match (interface.ext_addr, interface.iface_addr) {
        (Some(ext_addr), Some(iface_addr)) => {
            if ext_addr != iface_addr {
                return Err(anyhow!(
                    "Your PublicIP ({ext_addr}) differs from interface IP ({iface_addr}), meaning that probably you're on a NAT, which is not supported by host-gw backend"
                ));
            }
        }
        (None, _) => {
            return Err(anyhow!("No external IP address available"));
        }
        (_, None) => {
            return Err(anyhow!("No interface IP address available"));
        }
    }

    info!("Host-GW compatibility check passed");
    Ok(())
}
