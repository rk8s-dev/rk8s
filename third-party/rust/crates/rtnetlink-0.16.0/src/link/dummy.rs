// SPDX-License-Identifier: MIT

use crate::{link::LinkMessageBuilder, packet_route::link::InfoKind};

/// Represent dummy interface.
/// Example code on creating a dummy interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkDummy};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkDummy::new("dummy0").build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkDummy> for more detail.
#[derive(Debug)]
pub struct LinkDummy;

impl LinkDummy {
    /// Equal to `LinkMessageBuilder::<LinkDummy>::new()`
    pub fn new(name: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkDummy>::new(name)
    }
}

impl LinkMessageBuilder<LinkDummy> {
    /// Create [LinkMessageBuilder] for dummy interface type
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkDummy>::new_with_info_kind(InfoKind::Dummy)
            .name(name.to_string())
    }
}
