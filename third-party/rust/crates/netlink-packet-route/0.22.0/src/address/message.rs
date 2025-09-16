// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::{
    address::{AddressAttribute, AddressHeaderFlags, AddressScope},
    AddressFamily,
};

const ADDRESS_HEADER_LEN: usize = 8;

buffer!(AddressMessageBuffer(ADDRESS_HEADER_LEN) {
    family: (u8, 0),
    prefix_len: (u8, 1),
    flags: (u8, 2),
    scope: (u8, 3),
    index: (u32, 4..ADDRESS_HEADER_LEN),
    payload: (slice, ADDRESS_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> AddressMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct AddressMessage {
    pub header: AddressHeader,
    pub attributes: Vec<AddressAttribute>,
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct AddressHeader {
    pub family: AddressFamily,
    pub prefix_len: u8,
    pub flags: AddressHeaderFlags,
    pub scope: AddressScope,
    pub index: u32,
}

impl Emitable for AddressHeader {
    fn buffer_len(&self) -> usize {
        ADDRESS_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = AddressMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
        packet.set_prefix_len(self.prefix_len);
        packet.set_flags(self.flags.bits());
        packet.set_scope(self.scope.into());
        packet.set_index(self.index);
    }
}

impl Emitable for AddressMessage {
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

impl<T: AsRef<[u8]>> Parseable<AddressMessageBuffer<T>> for AddressHeader {
    fn parse(buf: &AddressMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            family: buf.family().into(),
            prefix_len: buf.prefix_len(),
            flags: AddressHeaderFlags::from_bits_retain(buf.flags()),
            scope: buf.scope().into(),
            index: buf.index(),
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<AddressMessageBuffer<&'a T>>
    for AddressMessage
{
    fn parse(buf: &AddressMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(AddressMessage {
            header: AddressHeader::parse(buf)
                .context("failed to parse address message header")?,
            attributes: Vec::<AddressAttribute>::parse(buf)
                .context("failed to parse address message NLAs")?,
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<AddressMessageBuffer<&'a T>>
    for Vec<AddressAttribute>
{
    fn parse(buf: &AddressMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        for nla_buf in buf.attributes() {
            attributes.push(AddressAttribute::parse(&nla_buf?)?);
        }
        Ok(attributes)
    }
}
