// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    DecodeError, Emitable, Parseable,
};

use crate::link::VlanProtocol;

const IFLA_VF_VLAN_INFO: u16 = 1;

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum VfVlan {
    Info(VfVlanInfo),
    Other(DefaultNla),
}

impl Nla for VfVlan {
    fn value_len(&self) -> usize {
        match self {
            Self::Info(v) => v.buffer_len(),
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Info(v) => v.emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Info(_) => IFLA_VF_VLAN_INFO,
            Self::Other(v) => v.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for VfVlan {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_VF_VLAN_INFO => Self::Info(
                VfVlanInfo::parse(&VfVlanInfoBuffer::new(payload)).context(
                    format!("invalid IFLA_VF_VLAN_INFO {payload:?}"),
                )?,
            ),
            kind => Self::Other(DefaultNla::parse(buf).context(format!(
                "failed to parse {kind} as DefaultNla: {payload:?}"
            ))?),
        })
    }
}

const VF_VLAN_INFO_LEN: usize = 16; // with 2 bytes padding

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfVlanInfo {
    pub vf_id: u32,
    pub vlan_id: u32,
    pub qos: u32,
    pub protocol: VlanProtocol,
}

impl VfVlanInfo {
    pub fn new(
        vf_id: u32,
        vlan_id: u32,
        qos: u32,
        protocol: VlanProtocol,
    ) -> Self {
        Self {
            vf_id,
            vlan_id,
            qos,
            protocol,
        }
    }
}

buffer!(VfVlanInfoBuffer(VF_VLAN_INFO_LEN) {
    vf_id: (u32, 0..4),
    vlan_id: (u32, 4..8),
    qos: (u32, 8..12),
    protocol: (u16, 12..14),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfVlanInfoBuffer<&T>> for VfVlanInfo {
    fn parse(buf: &VfVlanInfoBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            vf_id: buf.vf_id(),
            vlan_id: buf.vlan_id(),
            qos: buf.qos(),
            protocol: u16::from_be(buf.protocol()).into(),
        })
    }
}

impl Emitable for VfVlanInfo {
    fn buffer_len(&self) -> usize {
        VF_VLAN_INFO_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfVlanInfoBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_vlan_id(self.vlan_id);
        buffer.set_qos(self.qos);
        buffer.set_protocol(u16::from(self.protocol).to_be());
    }
}
