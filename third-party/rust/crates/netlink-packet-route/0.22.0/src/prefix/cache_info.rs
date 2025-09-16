// SPDX-License-Identifier: MIT

use netlink_packet_utils::{traits::Parseable, DecodeError, Emitable};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct CacheInfo {
    pub preferred_time: u32,
    pub valid_time: u32,
}

const CACHE_INFO_LEN: usize = 8;

buffer!(CacheInfoBuffer(CACHE_INFO_LEN) {
    preferred_time: (u32, 0..4),
    valid_time: (u32, 4..8),
});

impl<T: AsRef<[u8]>> Parseable<CacheInfoBuffer<T>> for CacheInfo {
    fn parse(buf: &CacheInfoBuffer<T>) -> Result<Self, DecodeError> {
        Ok(CacheInfo {
            preferred_time: buf.preferred_time(),
            valid_time: buf.valid_time(),
        })
    }
}

impl Emitable for CacheInfo {
    fn buffer_len(&self) -> usize {
        CACHE_INFO_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = CacheInfoBuffer::new(buffer);
        buffer.set_preferred_time(self.preferred_time);
        buffer.set_valid_time(self.valid_time);
    }
}
