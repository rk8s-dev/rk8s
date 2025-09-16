// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::AddressFamily;

const NEIGHBOUR_TABLE_HEADER_LEN: usize = 4;

buffer!(NeighbourTableMessageBuffer(NEIGHBOUR_TABLE_HEADER_LEN) {
    family: (u8, 0),
    payload: (slice, NEIGHBOUR_TABLE_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> NeighbourTableMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

// kernel code is `struct rtgenmsg`
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct NeighbourTableHeader {
    pub family: AddressFamily,
}

impl<T: AsRef<[u8]>> Parseable<NeighbourTableMessageBuffer<T>>
    for NeighbourTableHeader
{
    fn parse(
        buf: &NeighbourTableMessageBuffer<T>,
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            family: buf.family().into(),
        })
    }
}

impl Emitable for NeighbourTableHeader {
    fn buffer_len(&self) -> usize {
        NEIGHBOUR_TABLE_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = NeighbourTableMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
    }
}
