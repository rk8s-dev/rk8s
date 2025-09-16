// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    DecodeError, Emitable,
};

const PREFIX_HEADER_LEN: usize = 12;

buffer!(PrefixMessageBuffer(PREFIX_HEADER_LEN) {
    prefix_family: (u8, 0),
    pad1: (u8, 1),
    pad2: (u16, 2..4),
    ifindex: (i32, 4..8),
    prefix_type: (u8, 8),
    prefix_len: (u8, 9),
    flags: (u8, 10),
    pad3: (u8, 11),
    payload: (slice, PREFIX_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> PrefixMessageBuffer<&'a T> {
    pub fn nlas(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct PrefixHeader {
    pub prefix_family: u8,
    pub ifindex: i32,
    pub prefix_type: u8,
    pub prefix_len: u8,
    pub flags: u8,
}

impl Emitable for PrefixHeader {
    fn buffer_len(&self) -> usize {
        PREFIX_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = PrefixMessageBuffer::new(buffer);
        packet.set_prefix_family(self.prefix_family);
        packet.set_ifindex(self.ifindex);
        packet.set_prefix_type(self.prefix_type);
        packet.set_prefix_len(self.prefix_len);
        packet.set_flags(self.flags);
    }
}
