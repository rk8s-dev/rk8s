// SPDX-License-Identifier: MIT

use crate::{
    packet_route::link::{InfoData, InfoKind, InfoVeth},
    LinkMessageBuilder, LinkUnspec,
};

#[derive(Debug)]
/// Represent virtual ethernet interface.
/// Example code on creating a veth pair
/// ```no_run
/// use rtnetlink::{new_connection, LinkVeth};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkVeth::new("veth1", "veth1-peer").build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkVeth> for more detail.
pub struct LinkVeth;

impl LinkVeth {
    /// Equal to `LinkMessageBuilder::<LinkVeth>::new(name, peer)`
    pub fn new(name: &str, peer: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkVeth>::new(name, peer)
    }
}

impl LinkMessageBuilder<LinkVeth> {
    /// Create [LinkMessageBuilder] for VETH
    pub fn new(name: &str, peer: &str) -> Self {
        LinkMessageBuilder::<LinkVeth>::new_with_info_kind(InfoKind::Veth)
            .name(name.to_string())
            .peer(peer)
    }

    pub fn peer(mut self, peer: &str) -> Self {
        let peer_msg = LinkMessageBuilder::<LinkUnspec>::new()
            .name(peer.to_string())
            .build();

        self.info_data = Some(InfoData::Veth(InfoVeth::Peer(peer_msg)));
        self
    }
}
