// SPDX-License-Identifier: MIT

use std::{env, net::Ipv4Addr};

use ipnetwork::Ipv4Network;
use rtnetlink::{new_connection, Error, Handle, RouteMessageBuilder};

const TEST_TABLE_ID: u32 = 299;

#[tokio::main]
async fn main() -> Result<(), ()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        usage();
        return Ok(());
    }

    let dest: Ipv4Network = args[1].parse().unwrap_or_else(|_| {
        eprintln!("invalid destination");
        std::process::exit(1);
    });
    let gateway: Ipv4Network = args[2].parse().unwrap_or_else(|_| {
        eprintln!("invalid gateway");
        std::process::exit(1);
    });

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    if let Err(e) = add_route(&dest, &gateway, handle.clone()).await {
        eprintln!("{e}");
    } else {
        println!("Route has been added to table {TEST_TABLE_ID}");
    }
    Ok(())
}

async fn add_route(
    dest: &Ipv4Network,
    gateway: &Ipv4Network,
    handle: Handle,
) -> Result<(), Error> {
    let route = RouteMessageBuilder::<Ipv4Addr>::new()
        .destination_prefix(dest.ip(), dest.prefix())
        .gateway(gateway.ip())
        .table_id(TEST_TABLE_ID)
        .build();
    handle.route().add(route).execute().await?;
    Ok(())
}

fn usage() {
    eprintln!(
        "\
usage:
    cargo run --example add_route -- <destination>/<prefix_length> <gateway>

Note that you need to run this program as root:

    env CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' \\
        cargo run --example add_route -- <destination>/<prefix_length> \
        <gateway>"
    );
}
