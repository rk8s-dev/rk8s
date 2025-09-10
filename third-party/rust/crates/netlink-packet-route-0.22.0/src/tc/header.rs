// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::AddressFamily;

const TC_HEADER_LEN: usize = 20;

buffer!(TcMessageBuffer(TC_HEADER_LEN) {
    family: (u8, 0),
    pad1: (u8, 1),
    pad2: (u16, 2..4),
    index: (i32, 4..8),
    handle: (u32, 8..12),
    parent: (u32, 12..16),
    info: (u32, 16..TC_HEADER_LEN),
    payload: (slice, TC_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> TcMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct TcHeader {
    pub family: AddressFamily,
    // Interface index
    pub index: i32,
    // Qdisc handle
    pub handle: TcHandle,
    // Parent Qdisc
    pub parent: TcHandle,
    pub info: u32,
}

impl TcHeader {
    pub const TCM_IFINDEX_MAGIC_BLOCK: u32 = 0xFFFFFFFF;
}

impl Emitable for TcHeader {
    fn buffer_len(&self) -> usize {
        TC_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
        packet.set_index(self.index);
        packet.set_handle(self.handle.into());
        packet.set_parent(self.parent.into());
        packet.set_info(self.info);
    }
}

impl<T: AsRef<[u8]>> Parseable<TcMessageBuffer<T>> for TcHeader {
    fn parse(buf: &TcMessageBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            family: buf.family().into(),
            index: buf.index(),
            handle: buf.handle().into(),
            parent: buf.parent().into(),
            info: buf.info(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct TcHandle {
    pub major: u16,
    pub minor: u16,
}

impl TcHandle {
    pub const UNSPEC: Self = Self { major: 0, minor: 0 };
    pub const ROOT: Self = Self {
        major: u16::MAX,
        minor: u16::MAX,
    };
    pub const INGRESS: Self = Self {
        major: u16::MAX,
        minor: 0xfff1,
    };

    pub const CLSACT: Self = Self::INGRESS;

    pub const MIN_PRIORITY: u16 = 0xFFE0;
    pub const MIN_INGRESS: u16 = 0xFFF2;
    pub const MIN_EGRESS: u16 = 0xFFF3;
}

impl From<u32> for TcHandle {
    fn from(d: u32) -> Self {
        let bytes = d.to_be_bytes();
        Self {
            major: u16::from_be_bytes([bytes[0], bytes[1]]),
            minor: u16::from_be_bytes([bytes[2], bytes[3]]),
        }
    }
}

impl From<TcHandle> for u32 {
    fn from(v: TcHandle) -> u32 {
        let major_bytes = v.major.to_be_bytes();
        let minor_bytes = v.minor.to_be_bytes();
        u32::from_be_bytes([
            major_bytes[0],
            major_bytes[1],
            minor_bytes[0],
            minor_bytes[1],
        ])
    }
}

impl std::fmt::Display for TcHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.major, self.minor)
    }
}
