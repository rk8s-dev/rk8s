// SPDX-License-Identifier: MIT

use std::env;

use ipnetwork::Ipv4Network;
use rtnetlink::{new_connection, Error, Handle};

#[tokio::main]
async fn main() -> Result<(), ()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        usage();
        return Ok(());
    }

    let dst: Ipv4Network = args[1].parse().unwrap_or_else(|_| {
        eprintln!("invalid destination");
        std::process::exit(1);
    });

    let table_id: u32 = args[2].parse().unwrap_or_else(|_| {
        eprintln!("invalid route table ID");
        std::process::exit(1);
    });

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    if let Err(e) = add_rule(&dst, table_id, handle.clone()).await {
        eprintln!("{e}");
    } else {
        println!("Route rule has been added for {dst} and lookup {table_id}")
    }
    Ok(())
}

async fn add_rule(
    dst: &Ipv4Network,
    table_id: u32,
    handle: Handle,
) -> Result<(), Error> {
    let rule = handle.rule();
    rule.add()
        .v4()
        .destination_prefix(dst.ip(), dst.prefix())
        .table_id(table_id)
        .execute()
        .await?;

    Ok(())
}

fn usage() {
    eprintln!(
        "\
usage: 
    cargo run --example add_rule -- <destination>/<prefix_length> <table_id> 

Note that you need to run this program as root:

    env CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' \\
        cargo run --example add_rule -- <destination>/<prefix_length> \
        <table_id>"
    );
}
