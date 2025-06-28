use std::net::{IpAddr, Ipv4Addr};

use crate::ip::link::get_handle;

use anyhow::anyhow;
use futures::TryStreamExt;
use ipnetwork::{IpNetwork, Ipv4Network};
use log::debug;
use netlink_packet_route::{
    AddressFamily,
    address::{AddressAttribute, AddressFlags, AddressMessage, AddressScope, CacheInfo},
};

#[derive(Debug)]
pub struct Addr {
    pub ipnet: IpNetwork,
    pub label: String,
    pub flags: AddressFlags,
    pub scope: AddressScope,
    pub peer: Option<IpNetwork>,
    pub broadcast: Option<IpAddr>,
    pub cache_info: CacheInfo,
    pub link_index: u32,
}

impl Eq for Addr {}

impl PartialEq for Addr {
    fn eq(&self, other: &Self) -> bool {
        self.ipnet == other.ipnet
    }
}

impl Default for Addr {
    fn default() -> Self {
        Self {
            ipnet: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::UNSPECIFIED, 0).unwrap()),
            label: "".to_string(),
            flags: AddressFlags::empty(),
            scope: AddressScope::default(),
            peer: None,
            broadcast: None,
            cache_info: CacheInfo::default(),
            link_index: 0,
        }
    }
}

pub enum ReqType {
    Add,
    Del,
    Get,
    Change,
}

pub async fn addr_add(index: u32, address: IpAddr, prefix_len: u8) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let address_handle = handle.address();
    debug!("container ip add :{}", address);
    address_handle
        .add(index, address, prefix_len)
        .execute()
        .await?;
    debug!("container ip add success");
    Ok(())
}

pub async fn addr_del(index: u32, address: IpAddr) -> anyhow::Result<()> {
    let handle = get_handle()?.ok_or_else(|| anyhow!("Cannot get handle"))?;
    let address_handle = handle.address();

    let mut req = address_handle.get().set_address_filter(address);
    let msg = req.message_mut();
    msg.header.index = index;
    //let mut address_message = AddressMessage::default();
    //address_message.header.index = index;
    //address_handle.del(address_message).execute().await?;
    address_handle.del(msg.clone()).execute().await?;
    Ok(())
}

pub async fn addr_list(index: u32, family: AddressFamily) -> anyhow::Result<Vec<Addr>> {
    let handle = get_handle()?
        .ok_or_else(|| anyhow!("Cannot get handle"))?
        .address();

    let mut addresses = Vec::new();

    let mut stream = handle.get().set_link_index_filter(index).execute();

    while let Some(msg) = stream.try_next().await? {
        if msg.header.family != family {
            continue;
        }
        let addr = Addr::try_from(&msg)?;
        addresses.push(addr);
    }

    Ok(addresses)
}

impl TryFrom<&AddressMessage> for Addr {
    type Error = anyhow::Error;

    fn try_from(msg: &AddressMessage) -> Result<Self, Self::Error> {
        let mut addr = Addr {
            link_index: msg.header.index,
            ..Default::default()
        };
        let mut dst = None;
        let mut local = None;

        let family = msg.header.family;
        for attr in &msg.attributes {
            match attr {
                AddressAttribute::Address(ip) => {
                    let ip = *ip;
                    let prefix = msg.header.prefix_len;
                    dst = Some(IpNetwork::new(ip, prefix)?);
                }
                AddressAttribute::Local(ip) => {
                    let ip = *ip;
                    let prefix = msg.header.prefix_len;
                    local = Some(IpNetwork::new(ip, prefix)?);
                }
                AddressAttribute::Label(label) => {
                    addr.label = label.clone();
                }
                AddressAttribute::Broadcast(bcast) => {
                    addr.broadcast = Some(IpAddr::V4(*bcast));
                }
                AddressAttribute::CacheInfo(info) => {
                    addr.cache_info = *info;
                }
                AddressAttribute::Multicast(_) => {}
                AddressAttribute::Flags(flags) => {
                    addr.flags = *flags;
                }
                AddressAttribute::Other(_) => {}
                _ => {}
            }
        }
        #[allow(clippy::collapsible_if)]
        if let Some(local) = local {
            if family == AddressFamily::Inet {
                if let Some(d) = dst {
                    if d.ip() == local.ip() {
                        addr.ipnet = d;
                    }
                }
            }else {
                addr.ipnet = local;
                addr.peer = dst;
            }
        } else if let Some(dst) = dst {
            addr.ipnet = dst;
        }
        addr.scope = msg.header.scope;

        Ok(addr)
    }
}

#[cfg(test)]
mod tests {
    use crate::ip::link;
    use log::info;
    use std::net::Ipv4Addr;

    use super::*;

    #[tokio::test]
    async fn test_addr_add() {
        let link = link::link_by_name("vethhost").await.unwrap();
        let result = addr_add(
            link.header.index,
            IpAddr::V4(Ipv4Addr::new(198, 19, 249, 211)),
            16, // prefix_len
        )
        .await;
        println!("Result: {:?}", result);
        assert!(result.is_ok(), "addr_add failed with error: {:?}", result);
    }

    #[tokio::test]
    async fn test_addr_del() {
        let link = link::link_by_name("vethhost").await.unwrap();
        //let res = addr_del(link.header.index).await;
        let res = addr_add(
            link.header.index,
            IpAddr::V4(Ipv4Addr::new(198, 19, 249, 211)),
            16, // prefix_len
        )
        .await;
        println!("res: {:?}", res);
    }

    #[tokio::test]
    async fn test_addr_list() {
        let link = link::link_by_name("vethhost").await.unwrap();
        let result = addr_list(link.header.index, AddressFamily::Inet).await;
        println!("result: {:?}", result);
        match result {
            Ok(result) => {
                for addr in result {
                    info!("result: {:?}", addr);
                }
            }
            Err(err) => {
                info!("err: {:?}", err);
            }
        }
    }
}
