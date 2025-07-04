use std::net::IpAddr;

use super::addr::{self, Addr};

use anyhow::{anyhow, bail};
use cni_plugin::{macaddr::MacAddr, reply::Route};
use futures::TryStreamExt;
use log::debug;
use macaddr::MacAddr6;
use netlink_packet_route::{
    AddressFamily,
    link::{InfoBridgePort, InfoPortData, LinkAttribute, LinkFlags, LinkInfo, LinkMessage},
};
use rtnetlink::{Handle, RouteMessageBuilder, new_connection};

/// Establishes an rtnetlink connection and returns a handle.
/// Returns `Ok(Some(Handle))` if successful, or an error otherwise.
pub fn get_handle() -> anyhow::Result<Option<Handle>> {
    let (connection, handle, _) =
        new_connection().map_err(|e| anyhow!("Failed to create rtnetlink connection: {}", e))?;
    tokio::spawn(connection);
    Ok(Some(handle))
}

/// Retrieves a link (network interface) by its index.
///
/// # Arguments
/// * `index` - The index of the network interface.
///
/// # Returns
/// * `Ok(Some(LinkMessage))` if found.
/// * `Ok(None)` if the interface does not exist.
/// * `Err(anyhow::Error)` if an error occurs.
pub async fn link_by_index(index: u32) -> anyhow::Result<LinkMessage> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    let mut links = handle.link().get().match_index(index).execute();

    let link = links
        .try_next()
        .await?
        .ok_or_else(|| anyhow!("Link with index {} not found", index))?;

    Ok(link)
}

/// Retrieves a link (network interface) by its name.
///
/// # Arguments
/// * `name` - The name of the network interface.
///
/// # Returns
/// * `Ok(Some(LinkMessage))` if found.
/// * `Ok(None)` if the interface does not exist.
/// * `Err(anyhow::Error)` if an error occurs.
pub async fn link_by_name(name: &str) -> anyhow::Result<LinkMessage> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    let mut links = handle.link().get().match_name(name.to_string()).execute();

    let link = links
        .try_next()
        .await?
        .ok_or_else(|| anyhow!("Link with name {} not found", name))?;

    Ok(link)
}

/// Add a network link configuration.
///
/// # Arguments
/// * `msg` - The link message containing the updated configuration.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn add_link(msg: LinkMessage) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    handle.link().add(msg).execute().await?;

    Ok(())
}

/// Updates a network link configuration.
///
/// # Arguments
/// * `msg` - The link message containing the updated configuration.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn set_link(msg: LinkMessage) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    handle.link().set(msg).execute().await?;

    Ok(())
}

/// Delete a network link configuration.
///
/// # Arguments
/// * `msg` - The link message containing the updated configuration.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn del_link(msg: LinkMessage) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    handle.link().del(msg.header.index).execute().await?;

    Ok(())
}
/// Sets a port link configuration.
///
/// # Arguments
/// * `msg` - The link message containing the port configuration.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn set_port_link(msg: LinkMessage) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    handle
        .link()
        .set_port(msg)
        .execute()
        .await
        .map_err(|e| anyhow!("Failed to set port link: {}", e))?;

    Ok(())
}

/// Enables a network link.
///
/// # Arguments
/// * `link` - Reference to the link message.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn link_set_up(link: &LinkMessage) -> anyhow::Result<()> {
    let mut msg = LinkMessage::default();

    msg.header.index = link.header.index;
    msg.header.flags |= LinkFlags::Up;
    msg.header.change_mask |= LinkFlags::Up;

    set_link(msg)
        .await
        .map_err(|e| anyhow!("Failed to set up: {}", e))?;
    Ok(())
}

/// Disables a network link.
///
/// # Arguments
/// * `link` - Reference to the link message.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn link_set_down(link: &LinkMessage) -> anyhow::Result<()> {
    let mut msg = LinkMessage::default();

    msg.header.index = link.header.index;
    msg.header.flags &= !LinkFlags::Up;
    msg.header.change_mask |= LinkFlags::Up;

    set_link(msg)
        .await
        .map_err(|e| anyhow!("Failed to set down: {}", e))?;
    Ok(())
}

/// Assigns a master device to a network link.
///
/// # Arguments
/// * `link` - The link to be assigned to a master.
/// * `master` - The master link.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn link_set_master(link: &LinkMessage, master: &LinkMessage) -> anyhow::Result<()> {
    let mut msg = LinkMessage::default();
    msg.header.index = link.header.index;
    msg.attributes
        .push(LinkAttribute::Controller(master.header.index));

    set_link(msg)
        .await
        .map_err(|e| anyhow!("Failed to set master: {}", e))?;
    Ok(())
}

/// Enables or disables hairpin mode on a bridge port.
///
/// # Arguments
/// * `link` - The link representing the port.
/// * `enable` - `true` to enable hairpin mode, `false` to disable.
///
/// # Returns
/// * `Ok(())` on success.
/// * `Err(anyhow::Error)` on failure.
pub async fn link_set_hairpin(link: &LinkMessage, enable: bool) -> anyhow::Result<()> {
    let mut msg = LinkMessage::default();
    msg.header.index = link.header.index;
    let hairpin_attr =
        LinkAttribute::LinkInfo(vec![LinkInfo::PortData(InfoPortData::BridgePort(vec![
            InfoBridgePort::HairpinMode(enable),
        ]))]);
    msg.attributes.push(hairpin_attr);
    set_port_link(msg)
        .await
        .map_err(|e| anyhow!("Failed to set hairpin: {}", e))?;
    Ok(())
}

/// Extracts the MAC address from a list of link attributes.
///
/// # Arguments
/// * `attributes` - A reference to a slice of `LinkAttribute`s.
///
/// # Returns
/// * `Some(MacAddr)` if a valid MAC address is found.
/// * `None` if no MAC address is found.
pub fn get_mac_address(attributes: &[LinkAttribute]) -> Option<MacAddr> {
    attributes.iter().find_map(|attr| match attr {
        LinkAttribute::Address(mac_bytes) if mac_bytes.len() == 6 => {
            Some(MacAddr::from(MacAddr6::new(
                mac_bytes[0],
                mac_bytes[1],
                mac_bytes[2],
                mac_bytes[3],
                mac_bytes[4],
                mac_bytes[5],
            )))
        }
        _ => None,
    })
}

pub async fn route_add(route: Route) -> anyhow::Result<()> {
    if route.gw.is_none() {
        bail!("Route Gateway must be specified");
    }
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let route_handle = handle.route();

    let mut builder = RouteMessageBuilder::<IpAddr>::new();
    builder = builder
        .destination_prefix(route.dst.ip(), route.dst.prefix())?
        .gateway(route.gw.unwrap())?;
    debug!("route_builder:{:?}", builder);
    route_handle.add(builder.build()).execute().await?;

    Ok(())
}

pub async fn route_del(route: Route) -> anyhow::Result<()> {
    if route.gw.is_none() {
        bail!("gw can not be all none");
    }
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let route_handle = handle.route();

    let mut builder = RouteMessageBuilder::<IpAddr>::new();
    builder = builder
        .destination_prefix(route.dst.ip(), route.dst.prefix())?
        .gateway(route.gw.unwrap())?;

    route_handle.del(builder.build()).execute().await?;

    Ok(())
}

// DelLinkByName removes an interface link.
pub async fn del_link_by_name(if_name: &str) -> anyhow::Result<()> {
    let link = link_by_name(if_name).await.map_err(|e| anyhow!("{}", e))?;
    del_link(link).await?;
    Ok(())
}

// DelLinkByNameAddr remove an interface and returns its addresses
pub async fn del_link_by_name_addr(if_name: &str) -> anyhow::Result<Vec<Addr>> {
    let link = link_by_name(if_name).await.map_err(|e| anyhow!("{}", e))?;

    let addr = addr::addr_list(link.header.index, AddressFamily::Inet).await?;

    del_link(link).await?;

    Ok(addr)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_del_link() {
        let name = "mynet0";
        let result = del_link_by_name(name).await;
        println!("Result: {:?}", result);
        assert!(result.is_ok(), "del_link failed with error: {:?}", result);
    }
}
