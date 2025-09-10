// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

/// Byte/Packet throughput statistics
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub struct TcStatsBasic {
    /// number of seen bytes
    pub bytes: u64,
    /// number of seen packets
    pub packets: u32,
}

// real size is 12, but kernel is align to 64bits(8 bytes)
const STATS_BASIC_LEN: usize = 16;

buffer!(TcStatsBasicBuffer(STATS_BASIC_LEN) {
    bytes: (u64, 0..8),
    packets: (u32, 8..12),
});

impl<T: AsRef<[u8]>> Parseable<TcStatsBasicBuffer<T>> for TcStatsBasic {
    fn parse(buf: &TcStatsBasicBuffer<T>) -> Result<Self, DecodeError> {
        Ok(TcStatsBasic {
            bytes: buf.bytes(),
            packets: buf.packets(),
        })
    }
}

impl Emitable for TcStatsBasic {
    fn buffer_len(&self) -> usize {
        STATS_BASIC_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = TcStatsBasicBuffer::new(buffer);
        buffer.set_bytes(self.bytes);
        buffer.set_packets(self.packets);
    }
}
