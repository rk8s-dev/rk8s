// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use rtnetlink::{new_connection, Error, Handle, LinkXfrm};

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        usage();
        return Ok(());
    }
    let link_name = &args[1];

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    create_xfrm(handle, link_name.to_string())
        .await
        .map_err(|e| format!("{e}"))
}

async fn create_xfrm(handle: Handle, link_name: String) -> Result<(), Error> {
    let mut parent_links =
        handle.link().get().match_name(link_name.clone()).execute();
    if let Some(parent) = parent_links.try_next().await? {
        let request = handle
            .link()
            .add(LinkXfrm::new("my-xfrm", parent.header.index, 0x08).build());

        request.execute().await?
    } else {
        println!("no link {link_name} found");
    }
    Ok(())
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example create_xfrm -- <link name>

Note that you need to run this program as root. Instead of running cargo as
root, build the example normally:

    cargo build --example create_xfrm

Then find the binary in the target directory:

    cd target/debug/examples ; sudo ./create_xfrm <link_name>"
    );
}
