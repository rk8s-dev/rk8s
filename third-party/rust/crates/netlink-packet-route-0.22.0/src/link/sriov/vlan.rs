// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_VLAN_LEN: usize = 12;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoVlan {
    pub vf_id: u32,
    pub vlan_id: u32,
    pub qos: u32,
}

impl VfInfoVlan {
    pub fn new(vf_id: u32, vlan_id: u32, qos: u32) -> Self {
        Self {
            vf_id,
            vlan_id,
            qos,
        }
    }
}

buffer!(VfInfoVlanBuffer(VF_INFO_VLAN_LEN) {
    vf_id: (u32, 0..4),
    vlan_id: (u32, 4..8),
    qos: (u32, 8..12)
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoVlanBuffer<&T>> for VfInfoVlan {
    fn parse(buf: &VfInfoVlanBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            vf_id: buf.vf_id(),
            vlan_id: buf.vlan_id(),
            qos: buf.qos(),
        })
    }
}

impl Emitable for VfInfoVlan {
    fn buffer_len(&self) -> usize {
        VF_INFO_VLAN_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoVlanBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_vlan_id(self.vlan_id);
        buffer.set_qos(self.qos);
    }
}
