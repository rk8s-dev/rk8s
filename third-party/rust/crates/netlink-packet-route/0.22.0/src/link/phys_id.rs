// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

const MAX_PHYS_ITEM_ID_LEN: usize = 32;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub struct LinkPhysId {
    pub id: [u8; MAX_PHYS_ITEM_ID_LEN],
    pub len: usize,
}

impl Parseable<[u8]> for LinkPhysId {
    fn parse(buf: &[u8]) -> Result<Self, DecodeError> {
        let len = buf.len() % MAX_PHYS_ITEM_ID_LEN;
        let mut id = [0; MAX_PHYS_ITEM_ID_LEN];
        id[..len].copy_from_slice(&buf[..len]);
        Ok(Self { id, len })
    }
}

impl Emitable for LinkPhysId {
    fn buffer_len(&self) -> usize {
        self.len
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer[..self.len].copy_from_slice(&self.id[..self.len])
    }
}
