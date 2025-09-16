// SPDX-License-Identifier: MIT

use crate::{
    link::LinkMessageBuilder,
    packet_route::link::{InfoData, InfoKind, InfoMacVtap, MacVtapMode},
};

/// Represent MAC VTAP interface.
/// Example code on creating a MAC VTAP interface
/// ```no_run
/// use rtnetlink::{new_connection, packet_route::link::MacVtapMode,
///                 LinkMacVtap};
///
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(
///             LinkMacVtap::new("macvtap100", 10, MacVtapMode::Bridge)
///                 .up()
///                 .build(),
///         )
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkMacVtap> for more detail.
#[derive(Debug)]
pub struct LinkMacVtap;

impl LinkMacVtap {
    /// Wrapper of `LinkMessageBuilder::<LinkMacVtap>::new().link().mode()`
    pub fn new(
        name: &str,
        base_iface_index: u32,
        mode: MacVtapMode,
    ) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkMacVtap>::new(name)
            .link(base_iface_index)
            .mode(mode)
    }
}

impl LinkMessageBuilder<LinkMacVtap> {
    /// Create [LinkMessageBuilder] for Mac VTAP interface
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkMacVtap>::new_with_info_kind(InfoKind::MacVtap)
            .name(name.to_string())
    }

    pub fn append_info_data(mut self, info: InfoMacVtap) -> Self {
        if let InfoData::MacVtap(infos) = self
            .info_data
            .get_or_insert_with(|| InfoData::MacVtap(Vec::new()))
        {
            infos.push(info);
        }
        self
    }

    pub fn mode(self, mode: MacVtapMode) -> Self {
        self.append_info_data(InfoMacVtap::Mode(mode))
    }
}
