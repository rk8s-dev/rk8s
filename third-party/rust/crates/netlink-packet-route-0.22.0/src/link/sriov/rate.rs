// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_RATE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoRate {
    pub vf_id: u32,
    pub min_tx_rate: u32,
    pub max_tx_rate: u32,
}

impl VfInfoRate {
    pub fn new(vf_id: u32, min_tx_rate: u32, max_tx_rate: u32) -> Self {
        Self {
            vf_id,
            min_tx_rate,
            max_tx_rate,
        }
    }
}

buffer!(VfInfoRateBuffer(VF_INFO_RATE_LEN) {
    vf_id: (u32, 0..4),
    min_tx_rate: (u32, 4..8),
    max_tx_rate: (u32, 8..12)
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoRateBuffer<&T>> for VfInfoRate {
    fn parse(buf: &VfInfoRateBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            vf_id: buf.vf_id(),
            min_tx_rate: buf.min_tx_rate(),
            max_tx_rate: buf.max_tx_rate(),
        })
    }
}

impl Emitable for VfInfoRate {
    fn buffer_len(&self) -> usize {
        VF_INFO_RATE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoRateBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_min_tx_rate(self.min_tx_rate);
        buffer.set_max_tx_rate(self.max_tx_rate);
    }
}
