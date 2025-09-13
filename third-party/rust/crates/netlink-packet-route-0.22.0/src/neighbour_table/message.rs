// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{
    NeighbourTableAttribute, NeighbourTableHeader, NeighbourTableMessageBuffer,
};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct NeighbourTableMessage {
    pub header: NeighbourTableHeader,
    pub attributes: Vec<NeighbourTableAttribute>,
}

impl Emitable for NeighbourTableMessage {
    fn buffer_len(&self) -> usize {
        self.header.buffer_len() + self.attributes.as_slice().buffer_len()
    }

    fn emit(&self, buffer: &mut [u8]) {
        self.header.emit(buffer);
        self.attributes
            .as_slice()
            .emit(&mut buffer[self.header.buffer_len()..]);
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<NeighbourTableMessageBuffer<&'a T>>
    for NeighbourTableMessage
{
    fn parse(
        buf: &NeighbourTableMessageBuffer<&'a T>,
    ) -> Result<Self, DecodeError> {
        Ok(NeighbourTableMessage {
            header: NeighbourTableHeader::parse(buf)
                .context("failed to parse neighbour table message header")?,
            attributes: Vec::<NeighbourTableAttribute>::parse(buf)
                .context("failed to parse neighbour table message NLAs")?,
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<NeighbourTableMessageBuffer<&'a T>>
    for Vec<NeighbourTableAttribute>
{
    fn parse(
        buf: &NeighbourTableMessageBuffer<&'a T>,
    ) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        for nla_buf in buf.attributes() {
            attributes.push(NeighbourTableAttribute::parse(&nla_buf?)?);
        }
        Ok(attributes)
    }
}
