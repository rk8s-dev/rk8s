// SPDX-License-Identifier: MIT

use crate::{
    link::LinkMessageBuilder,
    packet_route::link::{InfoData, InfoKind, InfoMacVlan, MacVlanMode},
};

/// Represent MAC VLAN interface.
/// Example code on creating a MAC VLAN interface
/// ```no_run
/// use rtnetlink::{new_connection, packet_route::link::MacVlanMode,
///                 LinkMacVlan};
///
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(
///             LinkMacVlan::new("macvlan100", 10, MacVlanMode::Bridge)
///                 .up()
///                 .build(),
///         )
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkMacVlan> for more detail.
#[derive(Debug)]
pub struct LinkMacVlan;

impl LinkMacVlan {
    /// Wrapper of `LinkMessageBuilder::<LinkMacVlan>::new().link().mode()`
    pub fn new(
        name: &str,
        base_iface_index: u32,
        mode: MacVlanMode,
    ) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkMacVlan>::new(name)
            .link(base_iface_index)
            .mode(mode)
    }
}

impl LinkMessageBuilder<LinkMacVlan> {
    /// Create [LinkMessageBuilder] for MAC VLAN
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkMacVlan>::new_with_info_kind(InfoKind::MacVlan)
            .name(name.to_string())
    }

    pub fn append_info_data(mut self, info: InfoMacVlan) -> Self {
        if let InfoData::MacVlan(infos) = self
            .info_data
            .get_or_insert_with(|| InfoData::MacVlan(Vec::new()))
        {
            infos.push(info);
        }
        self
    }

    pub fn mode(self, mode: MacVlanMode) -> Self {
        self.append_info_data(InfoMacVlan::Mode(mode))
    }
}
