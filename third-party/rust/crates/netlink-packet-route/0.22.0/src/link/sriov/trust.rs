// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_TRUST_LEN: usize = 8;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoTrust {
    pub vf_id: u32,
    pub enabled: bool,
}

impl VfInfoTrust {
    pub fn new(vf_id: u32, enabled: bool) -> Self {
        Self { vf_id, enabled }
    }
}

buffer!(VfInfoTrustBuffer(VF_INFO_TRUST_LEN) {
    vf_id: (u32, 0..4),
    setting: (u32, 4..8),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoTrustBuffer<&T>> for VfInfoTrust {
    fn parse(buf: &VfInfoTrustBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(
            buf.vf_id(),
            buf.setting() > 0 && buf.setting() != u32::MAX,
        ))
    }
}

impl Emitable for VfInfoTrust {
    fn buffer_len(&self) -> usize {
        VF_INFO_TRUST_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoTrustBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_setting(self.enabled as u32);
    }
}
