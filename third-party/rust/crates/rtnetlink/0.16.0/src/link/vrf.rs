// SPDX-License-Identifier: MIT

use crate::{
    packet_route::link::{InfoData, InfoKind, InfoVrf},
    LinkMessageBuilder,
};

/// Represent VRF interface.
/// Example code on creating a VRF interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkVrf};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(
///             LinkVrf::new("my-vrf", 100)
///                 .up()
///                 .build()
///         )
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkVrf> for more detail.
#[derive(Debug)]
pub struct LinkVrf;

impl LinkVrf {
    /// Wrapper of `LinkMessageBuilder::<LinkVrf>::new().table_id()`
    pub fn new(name: &str, table_id: u32) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkVrf>::new(name).table_id(table_id)
    }
}

impl LinkMessageBuilder<LinkVrf> {
    /// Create [LinkMessageBuilder] for VRF
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkVrf>::new_with_info_kind(InfoKind::Vrf)
            .name(name.to_string())
    }

    pub fn append_info_data(mut self, info: InfoVrf) -> Self {
        if let InfoData::Vrf(infos) = self
            .info_data
            .get_or_insert_with(|| InfoData::Vrf(Vec::new()))
        {
            infos.push(info);
        }
        self
    }

    /// VRF table ID
    pub fn table_id(self, table_id: u32) -> Self {
        self.append_info_data(InfoVrf::TableId(table_id))
    }
}
