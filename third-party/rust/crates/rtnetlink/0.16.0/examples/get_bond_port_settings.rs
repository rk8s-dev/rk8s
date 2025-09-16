// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use rtnetlink::{
    new_connection, packet_route::link::LinkAttribute, Error, Handle,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), ()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        usage();
        return Ok(());
    }
    let link_name = &args[1];

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    let linkname = link_name.to_string();
    println!("dumping bond port settings for link \"{linkname}\"");

    if let Err(e) = dump_bond_port_settings(handle, linkname).await {
        eprintln!("{e}");
    }

    Ok(())
}

async fn dump_bond_port_settings(
    handle: Handle,
    linkname: String,
) -> Result<(), Error> {
    let mut links = handle.link().get().match_name(linkname.clone()).execute();
    if let Some(_link) = links.try_next().await? {
        let mut link_messgage =
            handle.link().get().match_name(linkname).execute();
        while let Some(msg) = link_messgage.try_next().await? {
            for nla in msg.attributes {
                if let LinkAttribute::LinkInfo(i) = &nla {
                    println!("{:?}", i);
                }
            }
        }
        Ok(())
    } else {
        eprintln!("link {linkname} not found");
        Ok(())
    }
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example get_bond_port_settings -- <link name>

Note that you need to run this program as root. Instead of running cargo as root,
build the example normally:

    cd netlink-ip ; cargo build --example get_bond_port_settings

Then find the binary in the target directory:

    cd ../target/debug/example ; sudo ./get_bond_port_settings <link_name>"
    );
}
