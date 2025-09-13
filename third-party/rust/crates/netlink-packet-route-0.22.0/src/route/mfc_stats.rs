// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct RouteMfcStats {
    pub packets: u64,
    pub bytes: u64,
    pub wrong_if: u64,
}

const MFC_STATS_LEN: usize = 24;

buffer!(RouteMfcStatsBuffer(MFC_STATS_LEN) {
    packets: (u64, 0..8),
    bytes: (u64, 8..16),
    wrong_if: (u64, 16..24),
});

impl<T: AsRef<[u8]>> Parseable<RouteMfcStatsBuffer<T>> for RouteMfcStats {
    fn parse(
        buf: &RouteMfcStatsBuffer<T>,
    ) -> Result<RouteMfcStats, DecodeError> {
        Ok(RouteMfcStats {
            packets: buf.packets(),
            bytes: buf.bytes(),
            wrong_if: buf.wrong_if(),
        })
    }
}

impl Emitable for RouteMfcStats {
    fn buffer_len(&self) -> usize {
        MFC_STATS_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = RouteMfcStatsBuffer::new(buffer);
        buffer.set_packets(self.packets);
        buffer.set_bytes(self.bytes);
        buffer.set_wrong_if(self.wrong_if);
    }
}
