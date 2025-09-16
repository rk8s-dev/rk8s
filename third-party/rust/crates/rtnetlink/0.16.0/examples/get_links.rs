// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use netlink_packet_route::{
    link::{LinkAttribute, LinkExtentMask},
    AddressFamily,
};
use rtnetlink::{new_connection, Error, Handle};

#[cfg(target_os = "freebsd")]
fn main() -> () {}

#[cfg(not(target_os = "freebsd"))]
#[tokio::main]
async fn main() -> Result<(), ()> {
    env_logger::init();
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    // Fetch a link by its index
    let index = 1;
    println!("*** retrieving link with index {index} ***");
    if let Err(e) = get_link_by_index(handle.clone(), index).await {
        eprintln!("{e}");
    }

    // Fetch a link by its name
    let name = "lo";
    println!("*** retrieving link named \"{name}\" ***");
    if let Err(e) = get_link_by_name(handle.clone(), name.to_string()).await {
        eprintln!("{e}");
    }

    // Dump all the links and print their index and name
    println!("*** dumping links ***");
    if let Err(e) = dump_links(handle.clone()).await {
        eprintln!("{e}");
    }

    // Dump all the bridge vlan information
    if let Err(e) = dump_bridge_filter_info(handle.clone()).await {
        eprintln!("{e}");
    }

    Ok(())
}

async fn get_link_by_index(handle: Handle, index: u32) -> Result<(), Error> {
    let mut links = handle.link().get().match_index(index).execute();
    let msg = if let Some(msg) = links.try_next().await? {
        msg
    } else {
        eprintln!("no link with index {index} found");
        return Ok(());
    };
    // We should have received only one message
    assert!(links.try_next().await?.is_none());

    for nla in msg.attributes.into_iter() {
        if let LinkAttribute::IfName(name) = nla {
            println!("found link with index {index} (name = {name})");
            return Ok(());
        }
    }
    eprintln!(
        "found link with index {index}, but this link does not have a name"
    );
    Ok(())
}

async fn get_link_by_name(handle: Handle, name: String) -> Result<(), Error> {
    let mut links = handle.link().get().match_name(name.clone()).execute();
    if (links.try_next().await?).is_some() {
        println!("found link {name}");
        // We should only have one link with that name
        assert!(links.try_next().await?.is_none());
    } else {
        println!("no link link {name} found");
    }
    Ok(())
}

async fn dump_links(handle: Handle) -> Result<(), Error> {
    let mut links = handle.link().get().execute();
    'outer: while let Some(msg) = links.try_next().await? {
        for nla in msg.attributes.into_iter() {
            if let LinkAttribute::IfName(name) = nla {
                println!("found link {} ({})", msg.header.index, name);
                continue 'outer;
            }
        }
        eprintln!("found link {}, but the link has no name", msg.header.index);
    }
    Ok(())
}

#[cfg(not(target_os = "freebsd"))]
async fn dump_bridge_filter_info(handle: Handle) -> Result<(), Error> {
    let mut links = handle
        .link()
        .get()
        .set_filter_mask(AddressFamily::Bridge, vec![LinkExtentMask::Brvlan])
        .execute();
    'outer: while let Some(msg) = links.try_next().await? {
        for nla in msg.attributes.into_iter() {
            if let LinkAttribute::AfSpecBridge(data) = nla {
                println!(
                    "found interface {} with AfSpecBridge data {:?})",
                    msg.header.index, data
                );
                continue 'outer;
            }
        }
    }
    Ok(())
}
