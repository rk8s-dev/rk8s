// SPDX-License-Identifier: MIT

use crate::{
    packet_route::link::{InfoData, InfoKind, InfoVlan, VlanQosMapping},
    LinkMessageBuilder,
};

/// A quality-of-service mapping between the internal priority `from` to the
/// external vlan priority `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QosMapping {
    pub from: u32,
    pub to: u32,
}

impl From<QosMapping> for VlanQosMapping {
    fn from(QosMapping { from, to }: QosMapping) -> Self {
        Self::Mapping(from, to)
    }
}

/// Represent VLAN interface.
/// Example code on creating a VLAN interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkVlan};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(
///             LinkVlan::new("vlan100", 10, 100)
///                 .up()
///                 .build()
///         )
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkVlan> for more detail.
#[derive(Debug)]
pub struct LinkVlan;

impl LinkVlan {
    /// Wrapper of `LinkMessageBuilder::<LinkVlan>::new().id().dev()`
    pub fn new(
        name: &str,
        base_iface_index: u32,
        vlan_id: u16,
    ) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkVlan>::new(name)
            .id(vlan_id)
            .link(base_iface_index)
    }
}

impl LinkMessageBuilder<LinkVlan> {
    /// Create [LinkMessageBuilder] for VLAN
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkVlan>::new_with_info_kind(InfoKind::Vlan)
            .name(name.to_string())
    }

    pub fn append_info_data(mut self, info: InfoVlan) -> Self {
        if let InfoData::Vlan(infos) = self
            .info_data
            .get_or_insert_with(|| InfoData::Vlan(Vec::new()))
        {
            infos.push(info);
        }
        self
    }

    /// VLAN ID
    pub fn id(self, vlan_id: u16) -> Self {
        self.append_info_data(InfoVlan::Id(vlan_id))
    }

    /// ingress QoS and egress QoS
    pub fn qos<I, E>(self, ingress_qos: I, egress_qos: E) -> Self
    where
        I: IntoIterator<Item = QosMapping>,
        E: IntoIterator<Item = QosMapping>,
    {
        let mut ret = self;
        let ingress: Vec<_> =
            ingress_qos.into_iter().map(VlanQosMapping::from).collect();
        if !ingress.is_empty() {
            ret = ret.append_info_data(InfoVlan::IngressQos(ingress));
        }

        let egress: Vec<_> =
            egress_qos.into_iter().map(VlanQosMapping::from).collect();

        if !egress.is_empty() {
            ret = ret.append_info_data(InfoVlan::EgressQos(egress));
        }
        ret
    }
}
