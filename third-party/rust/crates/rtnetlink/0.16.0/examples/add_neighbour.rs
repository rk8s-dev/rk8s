// SPDX-License-Identifier: MIT

use futures::stream::TryStreamExt;
use rtnetlink::{new_connection, Error, Handle};
use std::{convert::TryFrom, env, net::IpAddr};

#[tokio::main]
async fn main() -> Result<(), ()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        usage();
        return Ok(());
    }

    let link_name = &args[1];
    let ip: IpAddr = args[2].parse().unwrap_or_else(|_| {
        eprintln!("invalid IP address");
        std::process::exit(1);
    });

    let link_local_parts: Vec<u8> = args[3]
        .split(':')
        .map(|b| {
            u8::from_str_radix(b, 16).unwrap_or_else(|e| {
                eprintln!("invalid part of mac {}: {}", b, e);
                std::process::exit(1);
            })
        })
        .collect();

    let link_local_address = <[u8; 6]>::try_from(link_local_parts).unwrap_or_else(|_| {
        eprintln!("invalid mac address, please give it in the format of 56:78:90:ab:cd:ef");
        std::process::exit(1);
    });

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    if let Err(e) =
        add_neighbour(link_name, ip, link_local_address, handle.clone()).await
    {
        eprintln!("{e}");
    }
    Ok(())
}

async fn add_neighbour(
    link_name: &str,
    ip: IpAddr,
    link_local_address: [u8; 6],
    handle: Handle,
) -> Result<(), Error> {
    let mut links = handle
        .link()
        .get()
        .match_name(link_name.to_string())
        .execute();
    if let Some(link) = links.try_next().await? {
        handle
            .neighbours()
            .add(link.header.index, ip)
            .link_local_address(&link_local_address)
            .execute()
            .await?;
        println!("Done");
    }

    Ok(())
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example add_neighbour -- <link_name> <ip_address> <mac>

Note that you need to run this program as root. Instead of running cargo as root,
build the example normally:

    cd rtnetlink ; cargo build --example add_neighbour

Then find the binary in the target directory:

    cd ../target/debug/example ;
    sudo ./add_neighbour <link_name> <ip_address> <mac>"
    );
}
