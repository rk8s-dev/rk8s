// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_GUID_LEN: usize = 12;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoGuid {
    pub vf_id: u32,
    pub guid: u64,
}

impl VfInfoGuid {
    pub fn new(vf_id: u32, guid: u64) -> Self {
        Self { vf_id, guid }
    }
}

buffer!(VfInfoGuidBuffer(VF_INFO_GUID_LEN) {
    vf_id: (u32, 0..4),
    guid: (u64, 4..12),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoGuidBuffer<&T>> for VfInfoGuid {
    fn parse(buf: &VfInfoGuidBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(buf.vf_id(), buf.guid()))
    }
}

impl Emitable for VfInfoGuid {
    fn buffer_len(&self) -> usize {
        VF_INFO_GUID_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoGuidBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_guid(self.guid);
    }
}
