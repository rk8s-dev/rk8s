// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::nsid::{NsidAttribute, NsidHeader, NsidMessageBuffer};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct NsidMessage {
    pub header: NsidHeader,
    pub attributes: Vec<NsidAttribute>,
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<NsidMessageBuffer<&'a T>>
    for NsidMessage
{
    fn parse(buf: &NsidMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: NsidHeader::parse(buf)
                .context("failed to parse nsid message header")?,
            attributes: Vec::<NsidAttribute>::parse(buf)
                .context("failed to parse nsid message NLAs")?,
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<NsidMessageBuffer<&'a T>>
    for Vec<NsidAttribute>
{
    fn parse(buf: &NsidMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        for nla_buf in buf.attributes() {
            attributes.push(NsidAttribute::parse(&nla_buf?)?);
        }
        Ok(attributes)
    }
}

impl Emitable for NsidMessage {
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
