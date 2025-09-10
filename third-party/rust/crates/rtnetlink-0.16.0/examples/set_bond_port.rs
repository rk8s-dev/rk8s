// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use rtnetlink::{
    new_connection,
    packet_route::link::{BondMode, LinkAttribute},
    Handle, LinkBond, LinkBondPort, LinkDummy, LinkUnspec,
};

async fn create_bond_and_get_index(handle: &Handle) -> Result<u32, String> {
    handle
        .link()
        .add(
            LinkBond::new("my-bond0")
                .mode(BondMode::ActiveBackup)
                .up()
                .build(),
        )
        .execute()
        .await
        .map_err(|e| format!("{e}"))?;

    let mut bond_links = handle
        .link()
        .get()
        .match_name("my-bond0".to_string())
        .execute();
    if let Some(bond_link) =
        bond_links.try_next().await.map_err(|e| format!("{e}"))?
    {
        Ok(bond_link.header.index)
    } else {
        Err("failed to find my-bond0".into())
    }
}

async fn create_dummy_and_attach_to_bond(
    handle: &Handle,
    bond_index: u32,
) -> Result<u32, String> {
    handle
        .link()
        .add(LinkDummy::new("my-dummy0").build())
        .execute()
        .await
        .map_err(|e| format!("{e}"))?;

    handle
        .link()
        .set(
            LinkUnspec::new_with_name("my-dummy0")
                .controller(bond_index)
                .down()
                .build(),
        )
        .execute()
        .await
        .map_err(|e| format!("{e}"))?;

    let mut dummy_links = handle
        .link()
        .get()
        .match_name("my-dummy0".to_string())
        .execute();
    if let Some(dummy_link) =
        dummy_links.try_next().await.map_err(|e| format!("{e}"))?
    {
        Ok(dummy_link.header.index)
    } else {
        Err("failed to find my-bond0".into())
    }
}

async fn set_bond_port(handle: &Handle, port_index: u32) -> Result<(), String> {
    let message = LinkBondPort::new(port_index).queue_id(1).prio(2).build();

    handle
        .link()
        .set_port(message)
        .execute()
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    let bond_index = create_bond_and_get_index(&handle).await?;

    let port_index =
        create_dummy_and_attach_to_bond(&handle, bond_index).await?;
    set_bond_port(&handle, port_index)
        .await
        .map_err(|e| e.to_string())?;

    let mut dummy_links = handle
        .link()
        .get()
        .match_name("my-dummy0".to_string())
        .execute();
    if let Some(dummy_link) =
        dummy_links.try_next().await.map_err(|e| format!("{e}"))?
    {
        for nla in dummy_link.attributes {
            if let LinkAttribute::LinkInfo(i) = &nla {
                println!("{:?}", i);
            }
        }
        Ok(())
    } else {
        Err("failed to find my-bond0".into())
    }
}
