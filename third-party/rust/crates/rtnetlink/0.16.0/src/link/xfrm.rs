// SPDX-License-Identifier: MIT

use crate::{
    link::LinkMessageBuilder,
    packet_route::link::{InfoData, InfoKind, InfoXfrm},
};

/// Represent XFRM interface.
/// Example code on creating a XFRM interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkXfrm};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkXfrm::new("xfrm8", 9, 0x08).build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkXfrm> for more detail.
#[derive(Debug)]
pub struct LinkXfrm;

impl LinkXfrm {
    /// Equal to `LinkMessageBuilder::<LinkXfrm>::new().dev().if_id()`
    pub fn new(
        name: &str,
        base_iface_index: u32,
        if_id: u32,
    ) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkXfrm>::new(name)
            .dev(base_iface_index)
            .if_id(if_id)
    }
}

impl LinkMessageBuilder<LinkXfrm> {
    /// Create [LinkMessageBuilder] for XFRM
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkXfrm>::new_with_info_kind(InfoKind::Xfrm)
            .name(name.to_string())
    }

    pub fn append_info_data(mut self, info: InfoXfrm) -> Self {
        if let InfoData::Xfrm(infos) = self
            .info_data
            .get_or_insert_with(|| InfoData::Xfrm(Vec::new()))
        {
            infos.push(info);
        }
        self
    }

    /// This is equivalent to the `if_id IF_ID` in command
    /// `ip link add name NAME type xfrm if_id IF_ID`.
    pub fn if_id(self, if_id: u32) -> Self {
        self.append_info_data(InfoXfrm::IfId(if_id))
    }

    /// This is equivalent to the `dev PHYS_DEV` in command
    /// `ip link add name NAME type xfm dev PHYS_DEV`, only take the interface
    /// index.
    pub fn dev(self, iface_index: u32) -> Self {
        self.append_info_data(InfoXfrm::Link(iface_index))
    }
}
