// SPDX-License-Identifier: MIT

use std::{env, str::FromStr};

use futures::stream::TryStreamExt;
use macaddr::MacAddr;
use rtnetlink::{
    new_connection, packet_route::link::MacVlanMode, Error, Handle, LinkMacVlan,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 && args.len() != 3 {
        usage();
        return Ok(());
    }
    let link_name = &args[1];
    let mac: Option<Vec<u8>> = if args.len() == 3 {
        let mac_address_arg = args[2].to_string();
        let mac_address = MacAddr::from_str(mac_address_arg.as_str())
            .map_err(|e| format!("{e}"))?;
        Some(mac_address.as_bytes().into())
    } else {
        None
    };

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    create_macvlan(handle, link_name.to_string(), mac)
        .await
        .map_err(|e| format!("{e}"))
}

async fn create_macvlan(
    handle: Handle,
    link_name: String,
    mac_address: Option<Vec<u8>>,
) -> Result<(), Error> {
    let mut parent_links =
        handle.link().get().match_name(link_name.clone()).execute();
    if let Some(parent) = parent_links.try_next().await? {
        let mut builder = LinkMacVlan::new(
            "my-macvlan",
            parent.header.index,
            MacVlanMode::Bridge,
        );
        if let Some(mac) = mac_address {
            builder = builder.address(mac);
        }
        let message = builder.build();
        let request = handle.link().add(message);

        request.execute().await?
    } else {
        println!("no link {link_name} found");
    }
    Ok(())
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example create_macvlan -- <link name> [mac address]

Note that you need to run this program as root. Instead of running cargo as root,
build the example normally:

    cargo build --example create_macvlan

Then find the binary in the target directory:

    cd target/debug/examples ; sudo ./create_macvlan <link_name> [mac address]"
    );
}
