// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct RouteCacheInfo {
    pub clntref: u32,
    pub last_use: u32,
    pub expires: u32,
    pub error: u32,
    pub used: u32,
    pub id: u32,
    pub ts: u32,
    pub ts_age: u32,
}

const CACHE_INFO_LEN: usize = 32;

buffer!(RouteCacheInfoBuffer(CACHE_INFO_LEN) {
    clntref: (u32, 0..4),
    last_use: (u32, 4..8),
    expires: (u32, 8..12),
    error: (u32, 12..16),
    used: (u32, 16..20),
    id: (u32, 20..24),
    ts: (u32, 24..28),
    ts_age: (u32, 28..32),
});

impl<T: AsRef<[u8]>> Parseable<RouteCacheInfoBuffer<T>> for RouteCacheInfo {
    fn parse(buf: &RouteCacheInfoBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            clntref: buf.clntref(),
            last_use: buf.last_use(),
            expires: buf.expires(),
            error: buf.error(),
            used: buf.used(),
            id: buf.id(),
            ts: buf.ts(),
            ts_age: buf.ts_age(),
        })
    }
}

impl Emitable for RouteCacheInfo {
    fn buffer_len(&self) -> usize {
        CACHE_INFO_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = RouteCacheInfoBuffer::new(buffer);
        buffer.set_clntref(self.clntref);
        buffer.set_last_use(self.last_use);
        buffer.set_expires(self.expires);
        buffer.set_error(self.error);
        buffer.set_used(self.used);
        buffer.set_id(self.id);
        buffer.set_ts(self.ts);
        buffer.set_ts_age(self.ts_age);
    }
}
