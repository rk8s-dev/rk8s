// SPDX-License-Identifier: MIT

use crate::{link::LinkMessageBuilder, packet_route::link::InfoKind};

/// Represent dummy interface.
/// Example code on creating a linux bridge interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkBridge};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkBridge::new("br0").build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkBridge> for more detail.
#[derive(Debug)]
pub struct LinkBridge;

impl LinkBridge {
    /// Equal to `LinkMessageBuilder::<LinkBridge>::new().up()`
    pub fn new(name: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkBridge>::new(name).up()
    }
}

impl LinkMessageBuilder<LinkBridge> {
    /// Create [LinkMessageBuilder] for linux bridge
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkBridge>::new_with_info_kind(InfoKind::Bridge)
            .name(name.to_string())
    }
}
