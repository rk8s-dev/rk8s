// SPDX-License-Identifier: MIT

use crate::{
    packet_route::link::{InfoBondPort, InfoPortData, InfoPortKind},
    LinkMessageBuilder,
};

#[derive(Debug)]
pub struct LinkBondPort;

impl LinkBondPort {
    pub fn new(port_index: u32) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkBondPort>::default()
            .index(port_index)
            .set_port_kind(InfoPortKind::Bond)
    }
}

impl LinkMessageBuilder<LinkBondPort> {
    /// Append arbitrary [InfoBondPort]
    pub fn append_info_data(self, info: InfoBondPort) -> Self {
        let mut ret = self;

        if let InfoPortData::BondPort(infos) = ret
            .port_data
            .get_or_insert_with(|| InfoPortData::BondPort(Vec::new()))
        {
            infos.push(info);
        }

        ret
    }

    /// Adds the `queue_id` attribute to the bond port
    /// This is equivalent to
    /// `ip link set name NAME type bond_slave queue_id QUEUE_ID`.
    pub fn queue_id(self, queue_id: u16) -> Self {
        self.append_info_data(InfoBondPort::QueueId(queue_id))
    }

    /// Adds the `prio` attribute to the bond port
    /// This is equivalent to `ip link set name NAME type bond_slave prio PRIO`.
    pub fn prio(self, prio: i32) -> Self {
        self.append_info_data(InfoBondPort::Prio(prio))
    }
}
