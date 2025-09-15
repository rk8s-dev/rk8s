// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

/// Queuing statistics
#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub struct TcStatsQueue {
    /// queue length
    pub qlen: u32,
    /// backlog size of queue
    pub backlog: u32,
    /// number of dropped packets
    pub drops: u32,
    /// number of requeues
    pub requeues: u32,
    /// number of enqueues over the limit
    pub overlimits: u32,
}

const STATS_QUEUE_LEN: usize = 20;

buffer!(TcStatsQueueBuffer( STATS_QUEUE_LEN) {
    qlen: (u32, 0..4),
    backlog: (u32, 4..8),
    drops: (u32, 8..12),
    requeues: (u32, 12..16),
    overlimits: (u32, 16..20),
});

impl<T: AsRef<[u8]>> Parseable<TcStatsQueueBuffer<T>> for TcStatsQueue {
    fn parse(buf: &TcStatsQueueBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            qlen: buf.qlen(),
            backlog: buf.backlog(),
            drops: buf.drops(),
            requeues: buf.requeues(),
            overlimits: buf.overlimits(),
        })
    }
}

impl Emitable for TcStatsQueue {
    fn buffer_len(&self) -> usize {
        STATS_QUEUE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = TcStatsQueueBuffer::new(buffer);
        buffer.set_qlen(self.qlen);
        buffer.set_backlog(self.backlog);
        buffer.set_drops(self.drops);
        buffer.set_requeues(self.requeues);
        buffer.set_overlimits(self.overlimits);
    }
}
