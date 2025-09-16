// SPDX-License-Identifier: MIT

use netlink_packet_utils::nla::{NlaBuffer, NlasIterator};
use netlink_packet_utils::{DecodeError, Emitable, Parseable};

use crate::AddressFamily;

const TCA_HEADER_LEN: usize = 4;

buffer!(TcActionMessageBuffer(TCA_HEADER_LEN) {
    family: (u8, 0),
    pad1: (u8, 1),
    pad2: (u16, 2..TCA_HEADER_LEN),
    payload: (slice, TCA_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> TcActionMessageBuffer<&'a T> {
    /// Returns an iterator over the attributes of a `TcActionMessageBuffer`.
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

/// Header for a traffic control action message.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct TcActionMessageHeader {
    /// Address family (usually `AddressFamily::Unspec`).
    pub family: AddressFamily,
}

impl Emitable for TcActionMessageHeader {
    fn buffer_len(&self) -> usize {
        TCA_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcActionMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
    }
}

impl<T: AsRef<[u8]>> Parseable<TcActionMessageBuffer<T>>
    for TcActionMessageHeader
{
    fn parse(buf: &TcActionMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(TcActionMessageHeader {
            family: buf.family().into(),
        })
    }
}
