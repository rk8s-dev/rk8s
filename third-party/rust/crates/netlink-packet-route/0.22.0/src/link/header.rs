// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::{link::LinkLayerType, AddressFamily};

use super::link_flag::LinkFlags;

const LINK_HEADER_LEN: usize = 16;

buffer!(LinkMessageBuffer(LINK_HEADER_LEN) {
    interface_family: (u8, 0),
    reserved_1: (u8, 1),
    link_layer_type: (u16, 2..4),
    link_index: (u32, 4..8),
    flags: (u32, 8..12),
    change_mask: (u32, 12..LINK_HEADER_LEN),
    payload: (slice, LINK_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> LinkMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

/// High level representation of `RTM_GETLINK`, `RTM_SETLINK`, `RTM_NEWLINK` and
/// `RTM_DELLINK` messages headers.
///
/// These headers have the following structure:
///
/// ```no_rust
/// 0                8                16              24               32
/// +----------------+----------------+----------------+----------------+
/// |interface family|    reserved    |         link layer type         |
/// +----------------+----------------+----------------+----------------+
/// |                             link index                            |
/// +----------------+----------------+----------------+----------------+
/// |                               flags                               |
/// +----------------+----------------+----------------+----------------+
/// |                            change mask                            |
/// +----------------+----------------+----------------+----------------+
/// ```
///
/// `LinkHeader` exposes all these fields except for the "reserved" one.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct LinkHeader {
    /// Address family: one of the `AF_*` constants.
    /// The [AddressFamily] has `From<u8>` and `From<AddressFamily> for u8`
    /// implemented.
    pub interface_family: AddressFamily,
    /// Link index.
    pub index: u32,
    /// Link type. It should be set to one of the `ARPHRD_*`
    /// constants. The most common value is [LinkLayerType::Ether] for
    /// Ethernet.
    /// The LinkLayerType has `From<u16>` and `From<LinkLayerType> for u16`
    /// implemented.
    pub link_layer_type: LinkLayerType,
    /// State of the link, described by a combinations of `IFF_*`
    /// constants.
    pub flags: LinkFlags,
    /// Change mask for the `flags` field.
    pub change_mask: LinkFlags,
}

impl Emitable for LinkHeader {
    fn buffer_len(&self) -> usize {
        LINK_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = LinkMessageBuffer::new(buffer);
        packet.set_interface_family(u8::from(self.interface_family));
        packet.set_link_index(self.index);
        packet.set_change_mask(self.change_mask.bits());
        packet.set_link_layer_type(u16::from(self.link_layer_type));
        packet.set_flags(self.flags.bits());
    }
}

impl<T: AsRef<[u8]>> Parseable<LinkMessageBuffer<T>> for LinkHeader {
    fn parse(buf: &LinkMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            interface_family: buf.interface_family().into(),
            link_layer_type: buf.link_layer_type().into(),
            index: buf.link_index(),
            change_mask: LinkFlags::from_bits_retain(buf.change_mask()),
            flags: LinkFlags::from_bits_retain(buf.flags()),
        })
    }
}
