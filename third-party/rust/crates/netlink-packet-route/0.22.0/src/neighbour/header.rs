// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{flags::NeighbourFlags, NeighbourState};
use crate::{route::RouteType, AddressFamily};

const NEIGHBOUR_HEADER_LEN: usize = 12;

buffer!(NeighbourMessageBuffer(NEIGHBOUR_HEADER_LEN) {
    family: (u8, 0),
    ifindex: (u32, 4..8),
    state: (u16, 8..10),
    flags: (u8, 10),
    kind: (u8, 11),
    payload:(slice, NEIGHBOUR_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> NeighbourMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

/// Neighbour headers have the following structure:
///
/// ```no_rust
/// 0                8                16              24               32
/// +----------------+----------------+----------------+----------------+
/// |     family     |                     padding                      |
/// +----------------+----------------+----------------+----------------+
/// |                             link index                            |
/// +----------------+----------------+----------------+----------------+
/// |              state              |     flags      |     ntype      |
/// +----------------+----------------+----------------+----------------+
/// ```
///
/// `NeighbourHeader` exposes all these fields.
// Linux kernel struct `struct ndmsg`
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct NeighbourHeader {
    pub family: AddressFamily,
    pub ifindex: u32,
    /// Neighbour cache entry state.
    pub state: NeighbourState,
    /// Neighbour cache entry flags. It should be set to a combination
    /// of the `NTF_*` constants
    pub flags: NeighbourFlags,
    /// Neighbour cache entry type. It should be set to one of the
    /// `NDA_*` constants.
    pub kind: RouteType,
}

impl<T: AsRef<[u8]>> Parseable<NeighbourMessageBuffer<T>> for NeighbourHeader {
    fn parse(buf: &NeighbourMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            family: buf.family().into(),
            ifindex: buf.ifindex(),
            state: buf.state().into(),
            flags: NeighbourFlags::from_bits_retain(buf.flags()),
            kind: buf.kind().into(),
        })
    }
}

impl Emitable for NeighbourHeader {
    fn buffer_len(&self) -> usize {
        NEIGHBOUR_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = NeighbourMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
        packet.set_ifindex(self.ifindex);
        packet.set_state(self.state.into());
        packet.set_flags(self.flags.bits());
        packet.set_kind(self.kind.into());
    }
}
