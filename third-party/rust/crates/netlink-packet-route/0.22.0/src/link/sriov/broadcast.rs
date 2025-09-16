// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_BROADCAST_LEN: usize = 32;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoBroadcast {
    pub addr: [u8; VF_INFO_BROADCAST_LEN],
}

impl VfInfoBroadcast {
    pub fn new(addr: &[u8]) -> Self {
        let mut ret = Self::default();
        if addr.len() > VF_INFO_BROADCAST_LEN {
            ret.addr.copy_from_slice(&addr[..VF_INFO_BROADCAST_LEN])
        } else {
            ret.addr[..addr.len()].copy_from_slice(addr)
        }
        ret
    }
}

buffer!(VfInfoBroadcastBuffer(VF_INFO_BROADCAST_LEN) {
    addr: (slice, 0..VF_INFO_BROADCAST_LEN),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoBroadcastBuffer<&T>>
    for VfInfoBroadcast
{
    fn parse(buf: &VfInfoBroadcastBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(buf.addr()))
    }
}

impl Emitable for VfInfoBroadcast {
    fn buffer_len(&self) -> usize {
        VF_INFO_BROADCAST_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoBroadcastBuffer::new(buffer);
        buffer.addr_mut().copy_from_slice(&self.addr);
    }
}
