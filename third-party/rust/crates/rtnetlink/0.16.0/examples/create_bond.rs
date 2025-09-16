// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use rtnetlink::{new_connection, packet_route::link::BondMode, LinkBond};

#[tokio::main]
async fn main() -> Result<(), String> {
    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    let message = LinkBond::new("my-bond")
        .mode(BondMode::ActiveBackup)
        .miimon(100)
        .updelay(100)
        .downdelay(100)
        .min_links(2)
        .arp_ip_target(vec![
            Ipv4Addr::new(6, 6, 7, 7),
            Ipv4Addr::new(8, 8, 9, 10),
        ])
        .ns_ip6_target(vec![
            Ipv6Addr::new(0xfd01, 0, 0, 0, 0, 0, 0, 1),
            Ipv6Addr::new(0xfd02, 0, 0, 0, 0, 0, 0, 2),
        ])
        .up()
        .build();

    handle
        .link()
        .add(message)
        .execute()
        .await
        .map_err(|e| format!("{e}"))
}
