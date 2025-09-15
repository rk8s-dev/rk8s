// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const MAX_ADDR_LEN: usize = 32;

const VF_INFO_MAC_LEN: usize = MAX_ADDR_LEN + 4;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoMac {
    pub vf_id: u32,
    pub mac: [u8; MAX_ADDR_LEN],
}

impl VfInfoMac {
    pub fn new(vf_id: u32, mac: &[u8]) -> Self {
        let mut ret = Self {
            vf_id,
            ..Default::default()
        };
        if mac.len() >= MAX_ADDR_LEN {
            ret.mac.copy_from_slice(&mac[..MAX_ADDR_LEN]);
        } else {
            ret.mac[..mac.len()].copy_from_slice(mac);
        }
        ret
    }
}

buffer!(VfInfoMacBuffer(VF_INFO_MAC_LEN) {
    vf_id: (u32, 0..4),
    mac: (slice, 4..VF_INFO_MAC_LEN),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoMacBuffer<&T>> for VfInfoMac {
    fn parse(buf: &VfInfoMacBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(buf.vf_id(), buf.mac()))
    }
}

impl Emitable for VfInfoMac {
    fn buffer_len(&self) -> usize {
        VF_INFO_MAC_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoMacBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.mac_mut().copy_from_slice(&self.mac);
    }
}
