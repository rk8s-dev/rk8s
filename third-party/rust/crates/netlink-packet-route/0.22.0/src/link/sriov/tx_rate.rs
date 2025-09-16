// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_TX_RATE_LEN: usize = 8;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoTxRate {
    pub vf_id: u32,
    pub rate: u32,
}

impl VfInfoTxRate {
    pub fn new(vf_id: u32, rate: u32) -> Self {
        Self { vf_id, rate }
    }
}

buffer!(VfInfoTxRateBuffer(VF_INFO_TX_RATE_LEN) {
    vf_id: (u32, 0..4),
    rate: (u32, 4..8),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoTxRateBuffer<&T>>
    for VfInfoTxRate
{
    fn parse(buf: &VfInfoTxRateBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            vf_id: buf.vf_id(),
            rate: buf.rate(),
        })
    }
}

impl Emitable for VfInfoTxRate {
    fn buffer_len(&self) -> usize {
        VF_INFO_TX_RATE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoTxRateBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_rate(self.rate);
    }
}
