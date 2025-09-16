// SPDX-License-Identifier: MIT

use std::env;

use futures::stream::TryStreamExt;
use rtnetlink::{
    new_connection, packet_route::link::MacVtapMode, Error, Handle, LinkMacVtap,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        usage();
        return Ok(());
    }
    let link_name = &args[1];

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    create_macvtap(handle, link_name.to_string())
        .await
        .map_err(|e| format!("{e}"))
}

async fn create_macvtap(
    handle: Handle,
    link_name: String,
) -> Result<(), Error> {
    let mut parent_links =
        handle.link().get().match_name(link_name.clone()).execute();
    if let Some(parent) = parent_links.try_next().await? {
        let message = LinkMacVtap::new(
            "test_macvtap",
            parent.header.index,
            MacVtapMode::Bridge,
        )
        .build();

        let request = handle.link().add(message);
        request.execute().await?
    } else {
        println!("no link link {link_name} found");
    }
    Ok(())
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example create_macvtap -- <link name>

Note that you need to run this program as root. Instead of running cargo as root,
build the example normally:

    cd rtnetlink; cargo build --example create_macvtap

Then find the binary in the target directory:

    cd ../target/debug/example ; sudo ./create_macvtap <link_name>"
    );
}
