// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use tokio::runtime::Runtime;

use crate::{
    new_connection,
    packet_route::link::{
        InfoData, InfoKind, InfoMacVlan, InfoVrf, LinkAttribute, LinkInfo,
        LinkMessage, MacVlanMode,
    },
    Error, LinkHandle, LinkMacVlan, LinkVrf, LinkWireguard,
};

const IFACE_NAME: &str = "wg142"; // rand?

#[test]
fn create_get_delete_wg() {
    let rt = Runtime::new().unwrap();
    let handle = rt.block_on(_create_wg());
    assert!(handle.is_ok());
    let mut handle = handle.unwrap();
    let msg = rt.block_on(_get_iface(&mut handle, IFACE_NAME.to_owned()));
    assert!(msg.is_ok());
    let msg = msg.unwrap();
    assert!(has_nla(
        &msg,
        &LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Wireguard)])
    ));
    rt.block_on(_del_iface(&mut handle, msg.header.index))
        .unwrap();
}

#[test]
fn create_get_delete_macvlan() {
    const MACVLAN_IFACE_NAME: &str = "mvlan1";
    const LOWER_DEVICE_IDX: u32 = 2;
    const MACVLAN_MODE: MacVlanMode = MacVlanMode::Bridge;
    let mac_address = [2u8, 0, 0, 0, 0, 1];

    let rt = Runtime::new().unwrap();
    let handle = rt.block_on(_create_macvlan(
        MACVLAN_IFACE_NAME,
        LOWER_DEVICE_IDX, /* assuming there's always a network interface in
                           * the system ... */
        MACVLAN_MODE,
        mac_address.to_vec(),
    ));
    assert!(handle.is_ok());

    let mut handle = handle.unwrap();
    let msg =
        rt.block_on(_get_iface(&mut handle, MACVLAN_IFACE_NAME.to_owned()));
    assert!(msg.is_ok());
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::MacVlan),
            LinkInfo::Data(InfoData::MacVlan(vec![
                InfoMacVlan::Mode(MACVLAN_MODE),
                InfoMacVlan::Flags(0), // defaulted by the kernel
                InfoMacVlan::MacAddrCount(0), // defaulted by the kernel
                InfoMacVlan::BcQueueLen(1000), // defaulted by the kernel
                InfoMacVlan::BcQueueLenUsed(1000)  // defaulted by the kernel
            ]))
        ])
    ));
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::IfName(MACVLAN_IFACE_NAME.to_string())
    ));
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::Link(LOWER_DEVICE_IDX)
    ));
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::Address(mac_address.to_vec())
    ));

    rt.block_on(_del_iface(&mut handle, msg.unwrap().header.index))
        .unwrap();
}

#[test]
fn create_delete_vrf() {
    const VRF_IFACE_NAME: &str = "vrf2222";
    const VRF_TABLE: u32 = 2222;
    let rt = Runtime::new().unwrap();
    let handle = rt.block_on(_create_vrf(VRF_IFACE_NAME, VRF_TABLE));
    assert!(handle.is_ok());

    let mut handle = handle.unwrap();
    let msg = rt.block_on(_get_iface(&mut handle, VRF_IFACE_NAME.to_owned()));
    assert!(msg.is_ok());
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::IfName(VRF_IFACE_NAME.to_string())
    ));
    assert!(has_nla(
        msg.as_ref().unwrap(),
        &LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::Vrf),
            LinkInfo::Data(InfoData::Vrf(vec![InfoVrf::TableId(VRF_TABLE),]))
        ])
    ));

    rt.block_on(_del_iface(&mut handle, msg.unwrap().header.index))
        .unwrap();
}

fn has_nla(msg: &LinkMessage, nla: &LinkAttribute) -> bool {
    msg.attributes.iter().any(|x| x == nla)
}

async fn _create_wg() -> Result<LinkHandle, Error> {
    let (conn, handle, _) = new_connection().unwrap();
    tokio::spawn(conn);
    let link_handle = handle.link();
    link_handle
        .add(LinkWireguard::new(IFACE_NAME).build())
        .execute()
        .await?;
    Ok(link_handle)
}

async fn _get_iface(
    handle: &mut LinkHandle,
    iface_name: String,
) -> Result<LinkMessage, Error> {
    let mut links = handle.get().match_name(iface_name).execute();
    let msg = links.try_next().await?;
    msg.ok_or(Error::RequestFailed)
}

async fn _del_iface(handle: &mut LinkHandle, index: u32) -> Result<(), Error> {
    handle.del(index).execute().await
}

async fn _create_macvlan(
    name: &str,
    lower_device_index: u32,
    mode: MacVlanMode,
    mac: Vec<u8>,
) -> Result<LinkHandle, Error> {
    let (conn, handle, _) = new_connection().unwrap();
    tokio::spawn(conn);
    let link_handle = handle.link();
    let req = link_handle.add(
        LinkMacVlan::new(name, lower_device_index, mode)
            .address(mac)
            .build(),
    );
    req.execute().await?;
    Ok(link_handle)
}

async fn _create_vrf(name: &str, table: u32) -> Result<LinkHandle, Error> {
    let (conn, handle, _) = new_connection().unwrap();
    tokio::spawn(conn);
    let link_handle = handle.link();
    let req = link_handle.add(LinkVrf::new(name, table).build());
    req.execute().await?;
    Ok(link_handle)
}
