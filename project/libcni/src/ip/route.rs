use std::net::IpAddr;

use crate::ip::link::get_handle;
use anyhow::{Result, anyhow};
use bitflags::bitflags;
use futures::TryStreamExt;
use ipnetwork::IpNetwork;
use log::{debug, info};
use macaddr::{MacAddr, MacAddr6, MacAddr8};
use netlink_packet_route::{
    AddressFamily,
    link::LinkAttribute,
    route::{RouteAddress, RouteAttribute, RouteMessage, RouteType},
};
use rtnetlink::RouteMessageBuilder;
use serde::{Deserialize, Serialize};
use serde_with::{FromInto, serde_as};

#[serde_as]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub dst: Option<IpNetwork>,
    pub oif_index: Option<u32>,
    pub gateway: Option<IpAddr>,
    pub src: Option<IpAddr>,
    #[serde_as(as = "Option<FromInto<u8>>")]
    pub route_type: Option<RouteType>,
    pub metric: Option<u32>,
}

pub fn route_equal(x: &Route, y: &Route) -> bool {
    x.dst == y.dst && x.gateway == y.gateway && x.oif_index == y.oif_index
}

pub async fn route_list(family: AddressFamily) -> Result<Vec<Route>> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let mut filter_msg = RouteMessage::default();
    filter_msg.header.address_family = family;
    let stream = handle.route().get(filter_msg).execute();
    collect_routes_from_stream(stream).await
}

#[derive(Debug, Clone, Default)]
pub struct RouteGetOptions {
    pub iif: Option<String>,
    pub iif_index: Option<u32>,
    pub oif: Option<String>,
    pub oif_index: Option<u32>,
    pub vrf_name: Option<String>,
    pub src_addr: Option<IpAddr>,
    pub uid: Option<u32>,
    pub mark: Option<u32>,
    pub fib_match: bool,
}

pub async fn route_get_with_options(
    dest: IpAddr,
    options: Option<&RouteGetOptions>,
) -> Result<Vec<Route>> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    let (family, prefix_len) = match dest {
        IpAddr::V4(_) => (AddressFamily::Inet, 32),
        IpAddr::V6(_) => (AddressFamily::Inet6, 128),
    };

    let mut msg = RouteMessage::default();
    msg.header.address_family = family;
    msg.header.destination_prefix_length = prefix_len;

    msg.attributes.push(RouteAttribute::Destination(match dest {
        IpAddr::V4(ipv4) => RouteAddress::Inet(ipv4),
        IpAddr::V6(ipv6) => RouteAddress::Inet6(ipv6),
    }));

    if let Some(opts) = options {
        if let Some(iif) = opts.iif_index {
            msg.attributes.push(RouteAttribute::Iif(iif));
        }
        if let Some(oif) = opts.oif_index {
            msg.attributes.push(RouteAttribute::Oif(oif));
        }
        if let Some(src) = opts.src_addr {
            msg.attributes.push(RouteAttribute::Source(match src {
                IpAddr::V4(ipv4) => RouteAddress::Inet(ipv4),
                IpAddr::V6(ipv6) => RouteAddress::Inet6(ipv6),
            }));
        }
        if let Some(uid) = opts.uid {
            msg.attributes.push(RouteAttribute::Uid(uid));
        }
        if let Some(mark) = opts.mark {
            msg.attributes.push(RouteAttribute::Mark(mark));
        }
    }

    let stream = handle.route().get(msg).execute();
    collect_routes_from_stream(stream).await
}

pub async fn collect_routes_from_stream<S>(mut stream: S) -> Result<Vec<Route>>
where
    S: TryStreamExt<Ok = RouteMessage> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut result = Vec::new();

    while let Some(reply) = stream.try_next().await? {
        let mut dst_ip: Option<IpAddr> = None;
        let mut oif_index: Option<u32> = None;
        let mut gateway: Option<IpAddr> = None;
        let mut src_ip: Option<IpAddr> = None;
        let mut metric: Option<u32> = None;

        for attr in &reply.attributes {
            match attr {
                RouteAttribute::Destination(RouteAddress::Inet(ip)) => {
                    dst_ip = Some(IpAddr::V4(*ip));
                }
                RouteAttribute::Destination(RouteAddress::Inet6(ip)) => {
                    dst_ip = Some(IpAddr::V6(*ip));
                }
                RouteAttribute::Oif(index) => {
                    oif_index = Some(*index);
                }
                RouteAttribute::Gateway(RouteAddress::Inet(ip)) => {
                    gateway = Some(IpAddr::V4(*ip));
                }
                RouteAttribute::Gateway(RouteAddress::Inet6(ip)) => {
                    gateway = Some(IpAddr::V6(*ip));
                }
                RouteAttribute::PrefSource(RouteAddress::Inet(ip)) => {
                    src_ip = Some(IpAddr::V4(*ip));
                }
                RouteAttribute::PrefSource(RouteAddress::Inet6(ip)) => {
                    src_ip = Some(IpAddr::V6(*ip));
                }
                RouteAttribute::Priority(pri) => {
                    metric = Some(*pri);
                }
                _ => {}
            }
        }

        let dst = match dst_ip {
            Some(ip) => Some(IpNetwork::new(ip, reply.header.destination_prefix_length)?),
            None => None,
        };

        result.push(Route {
            dst,
            oif_index,
            gateway,
            src: src_ip,
            route_type: Some(reply.header.kind),
            metric,
        });
    }
    Ok(result)
}

pub async fn route_add(route: Route) -> anyhow::Result<()> {
    let gateway = route
        .gateway
        .ok_or_else(|| anyhow!("Route Gateway must be specified"))?;
    let dst = route
        .dst
        .ok_or_else(|| anyhow!("Route destination must be specified"))?;
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let route_handle = handle.route();

    let mut builder = RouteMessageBuilder::<IpAddr>::new();
    builder = builder
        .destination_prefix(dst.ip(), dst.prefix())?
        .gateway(gateway)?;
    if let Some(pri) = route.metric {
        builder = builder.priority(pri);
    }
    debug!("route_builder:{builder:?}");
    route_handle.add(builder.build()).execute().await?;

    Ok(())
}

pub async fn route_del(route: Route) -> anyhow::Result<()> {
    let gateway = route
        .gateway
        .ok_or_else(|| anyhow!("Route Gateway must be specified"))?;
    let dst = route
        .dst
        .ok_or_else(|| anyhow!("Route destination must be specified"))?;
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let route_handle = handle.route();

    let mut builder = RouteMessageBuilder::<IpAddr>::new();
    builder = builder
        .destination_prefix(dst.ip(), dst.prefix())?
        .gateway(gateway)?;
    info!("route_builder:{builder:?}");
    route_handle.del(builder.build()).execute().await?;

    Ok(())
}

pub async fn route_get(dest: IpAddr) -> Result<Vec<Route>> {
    route_get_with_options(dest, None).await
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RouteFilterMask {
    pub dst: bool,
    pub oif_index: bool,
    pub gateway: bool,
    pub src: bool,
    pub route_type: bool,
    pub metric: bool,
}

pub async fn route_list_filtered<F>(
    family: AddressFamily,
    filter: Option<&Route>,
    mask: RouteFilterMask,
    mut f: F,
) -> Result<()>
where
    F: FnMut(Route) -> bool,
{
    let routes = route_list(family).await?;

    for route in routes {
        if let Some(filter) = filter {
            if mask.dst && route.dst != filter.dst {
                continue;
            }
            if mask.oif_index && route.oif_index != filter.oif_index {
                continue;
            }
            if mask.gateway && route.gateway != filter.gateway {
                continue;
            }
            if mask.src && route.src != filter.src {
                continue;
            }
            if mask.route_type && route.route_type != filter.route_type {
                continue;
            }
            if mask.metric && route.metric != filter.metric {
                continue;
            }
        }

        if !f(route) {
            break;
        }
    }
    Ok(())
}

pub async fn route_list_filtered_vec(
    family: AddressFamily,
    filter: Option<&Route>,
    mask: RouteFilterMask,
) -> Result<Vec<Route>> {
    let mut result = Vec::new();

    route_list_filtered(family, filter, mask, |route| {
        result.push(route);
        true
    })
    .await?;

    Ok(result)
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Flags: u32 {
        const UP             = 1 << 0;
        const BROADCAST      = 1 << 1;
        const LOOPBACK       = 1 << 2;
        const POINT_TO_POINT = 1 << 3;
        const MULTICAST      = 1 << 4;
        const RUNNING        = 1 << 5;
    }
}

#[derive(Debug, Clone)]
pub struct Interface {
    pub index: u32,
    pub mtu: Option<u32>,
    pub name: String,
    pub hardware_addr: Option<MacAddr>,
    pub flags: Flags,
}

impl Default for Interface {
    fn default() -> Self {
        Self {
            index: 0,
            mtu: None,
            name: String::new(),
            hardware_addr: None,
            flags: Flags::empty(),
        }
    }
}

impl Interface {
    pub fn new(
        index: u32,
        name: String,
        mtu: Option<u32>,
        hardware_addr: Option<MacAddr>,
        flags: Flags,
    ) -> Self {
        Self {
            index,
            mtu,
            name,
            hardware_addr,
            flags,
        }
    }
}

fn parse_mac_addr(bytes: &[u8]) -> Option<MacAddr> {
    match bytes.len() {
        6 => {
            let arr: [u8; 6] = bytes.try_into().ok()?;
            Some(MacAddr::V6(MacAddr6::new(
                arr[0], arr[1], arr[2], arr[3], arr[4], arr[5],
            )))
        }
        8 => {
            let arr: [u8; 8] = bytes.try_into().ok()?;
            Some(MacAddr::V8(MacAddr8::new(
                arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
            )))
        }
        _ => None,
    }
}

pub async fn interface_table(ifindex: u32) -> anyhow::Result<Vec<Interface>> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;

    let mut links = handle.link().get().execute();
    let mut results = Vec::new();

    while let Some(msg) = links.try_next().await? {
        if ifindex != 0 && msg.header.index != ifindex {
            continue;
        }

        let mut name = String::new();
        let mut mtu = None;
        let mut hwaddr = None;

        for attr in msg.attributes {
            match attr {
                LinkAttribute::IfName(n) => name = n,
                LinkAttribute::Mtu(m) => mtu = Some(m),
                LinkAttribute::Address(addr) => hwaddr = parse_mac_addr(&addr),
                _ => {}
            }
        }

        results.push(Interface {
            index: msg.header.index,
            name,
            mtu,
            hardware_addr: hwaddr,
            flags: Flags::from_bits_truncate(msg.header.flags.bits()),
        });

        if ifindex != 0 {
            break;
        }
    }

    Ok(results)
}

pub async fn interfaces() -> Result<Vec<Interface>> {
    interface_table(0)
        .await
        .map_err(|e| anyhow!("route: ip+net: {}", e))
}

pub async fn interface_by_index(index: u32) -> Result<Interface> {
    if index == 0 {
        return Err(anyhow!("invalid interface index: {}", index));
    }

    let ift = interface_table(index)
        .await
        .map_err(|e| anyhow!("route: ip+net: {}", e))?;

    for iface in &ift {
        if iface.index == index {
            return Ok(iface.clone());
        }
    }

    Err(anyhow!("no such interface with index {}", index))
}

pub async fn interface_by_name(name: String) -> Result<Interface> {
    if name.is_empty() {
        return Err(anyhow!("invalid interface name: {}", name));
    }

    let ift = interfaces().await?;

    for iface in &ift {
        if iface.name == name {
            return Ok(iface.clone());
        }
    }

    Err(anyhow!("no such interface with name {}", name))
}
