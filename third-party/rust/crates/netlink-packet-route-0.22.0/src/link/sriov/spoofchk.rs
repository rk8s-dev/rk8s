// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_SPOOFCHK_LEN: usize = 8;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoSpoofCheck {
    pub vf_id: u32,
    pub enabled: bool,
}

impl VfInfoSpoofCheck {
    pub fn new(vf_id: u32, enabled: bool) -> Self {
        Self { vf_id, enabled }
    }
}

buffer!(VfInfoSpoofCheckBuffer(VF_INFO_SPOOFCHK_LEN) {
    vf_id: (u32, 0..4),
    setting: (u32, 4..8),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoSpoofCheckBuffer<&T>>
    for VfInfoSpoofCheck
{
    fn parse(buf: &VfInfoSpoofCheckBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(
            buf.vf_id(),
            buf.setting() > 0 && buf.setting() != u32::MAX,
        ))
    }
}

impl Emitable for VfInfoSpoofCheck {
    fn buffer_len(&self) -> usize {
        VF_INFO_SPOOFCHK_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoSpoofCheckBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_setting(self.enabled as u32);
    }
}
