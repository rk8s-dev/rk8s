// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use crate::link::{LinkAttribute, LinkHeader, LinkMessageBuffer};
use crate::AddressFamily;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct LinkMessage {
    pub header: LinkHeader,
    pub attributes: Vec<LinkAttribute>,
}

impl Emitable for LinkMessage {
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

impl<'a, T: AsRef<[u8]> + 'a> Parseable<LinkMessageBuffer<&'a T>>
    for LinkMessage
{
    fn parse(buf: &LinkMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let header = LinkHeader::parse(buf)
            .context("failed to parse link message header")?;
        let interface_family = header.interface_family;
        let attributes =
            Vec::<LinkAttribute>::parse_with_param(buf, interface_family)
                .context("failed to parse link message NLAs")?;
        Ok(LinkMessage { header, attributes })
    }
}

impl<'a, T: AsRef<[u8]> + 'a>
    ParseableParametrized<LinkMessageBuffer<&'a T>, AddressFamily>
    for Vec<LinkAttribute>
{
    fn parse_with_param(
        buf: &LinkMessageBuffer<&'a T>,
        family: AddressFamily,
    ) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        for nla_buf in buf.attributes() {
            attributes
                .push(LinkAttribute::parse_with_param(&nla_buf?, family)?);
        }
        Ok(attributes)
    }
}
