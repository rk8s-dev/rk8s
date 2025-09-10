// SPDX-License-Identifier: MIT

use crate::{link::LinkMessageBuilder, packet_route::link::InfoKind};

/// Represent wireguard interface.
/// Example code on creating a wireguard interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkWireguard};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkWireguard::new("wg0").build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkWireguard> for more detail.
#[derive(Debug)]
pub struct LinkWireguard;

impl LinkWireguard {
    /// Equal to `LinkMessageBuilder::<LinkWireguard>::new()`
    pub fn new(name: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkWireguard>::new(name)
    }
}

impl LinkMessageBuilder<LinkWireguard> {
    /// Create [LinkMessageBuilder] for wireguard
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkWireguard>::new_with_info_kind(
            InfoKind::Wireguard,
        )
        .name(name.to_string())
    }
}
