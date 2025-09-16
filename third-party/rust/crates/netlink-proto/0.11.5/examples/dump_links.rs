// SPDX-License-Identifier: MIT

use futures::StreamExt;
use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_route::{link::LinkMessage, RouteNetlinkMessage};
use netlink_proto::{
    new_connection,
    sys::{protocols::NETLINK_ROUTE, SocketAddr},
};

#[tokio::main]
async fn main() -> Result<(), String> {
    // Create the netlink socket. Here, we won't use the channel that
    // receives unsolicited messages.
    let (conn, handle, _) = new_connection(NETLINK_ROUTE).map_err(|e| {
        format!("Failed to create a new netlink connection: {e}")
    })?;

    // Spawn the `Connection` in the background
    tokio::spawn(conn);

    // Create the netlink message that requests the links to be dumped
    let mut nl_hdr = NetlinkHeader::default();
    nl_hdr.flags = NLM_F_DUMP | NLM_F_REQUEST;
    let request = NetlinkMessage::new(
        nl_hdr,
        RouteNetlinkMessage::GetLink(LinkMessage::default()).into(),
    );

    // Send the request
    let mut response = handle
        .request(request, SocketAddr::new(0, 0))
        .map_err(|e| format!("Failed to send request: {e}"))?;

    // Print all the messages received in response
    while let Some(packet) = response.next().await {
        println!("<<< {packet:?}");
    }

    Ok(())
}
