// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct NeighbourCacheInfo {
    pub confirmed: u32,
    pub used: u32,
    pub updated: u32,
    pub refcnt: u32,
}

const NEIGHBOUR_CACHE_INFO_LEN: usize = 16;

buffer!(NeighbourCacheInfoBuffer(NEIGHBOUR_CACHE_INFO_LEN) {
    confirmed: (u32, 0..4),
    used: (u32, 4..8),
    updated: (u32, 8..12),
    refcnt: (u32, 12..16),
});

impl<T: AsRef<[u8]>> Parseable<NeighbourCacheInfoBuffer<T>>
    for NeighbourCacheInfo
{
    fn parse(buf: &NeighbourCacheInfoBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            confirmed: buf.confirmed(),
            used: buf.used(),
            updated: buf.updated(),
            refcnt: buf.refcnt(),
        })
    }
}

impl Emitable for NeighbourCacheInfo {
    fn buffer_len(&self) -> usize {
        NEIGHBOUR_CACHE_INFO_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = NeighbourCacheInfoBuffer::new(buffer);
        buffer.set_confirmed(self.confirmed);
        buffer.set_used(self.used);
        buffer.set_updated(self.updated);
        buffer.set_refcnt(self.refcnt);
    }
}
