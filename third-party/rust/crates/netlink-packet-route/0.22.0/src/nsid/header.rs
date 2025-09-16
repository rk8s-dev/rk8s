// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    DecodeError, Emitable, Parseable,
};

use crate::AddressFamily;

const NSID_HEADER_LEN: usize = 4;

buffer!(NsidMessageBuffer(NSID_HEADER_LEN) {
    family: (u8, 0),
    payload: (slice, NSID_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> NsidMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct NsidHeader {
    pub family: AddressFamily,
}

impl Emitable for NsidHeader {
    fn buffer_len(&self) -> usize {
        NSID_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = NsidMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
    }
}

impl<T: AsRef<[u8]>> Parseable<NsidMessageBuffer<T>> for NsidHeader {
    fn parse(buf: &NsidMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(NsidHeader {
            family: buf.family().into(),
        })
    }
}
