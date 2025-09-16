// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::NlaBuffer, DecodeError, Emitable, ParseableParametrized,
};

use crate::link::InfoKind;

// This is filled by driver via `struct rtnl_link_ops.fill_xstats`
// Currently(Linux kernel 6.5.8), only the `can` interface support so.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum LinkXstats {
    Other(Vec<u8>),
}

impl Emitable for LinkXstats {
    fn buffer_len(&self) -> usize {
        match self {
            Self::Other(v) => v.len(),
        }
    }

    fn emit(&self, buffer: &mut [u8]) {
        match self {
            Self::Other(v) => buffer.copy_from_slice(v.as_slice()),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized>
    ParseableParametrized<NlaBuffer<&'a T>, &InfoKind> for LinkXstats
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        _kind: &InfoKind,
    ) -> Result<Self, DecodeError> {
        Ok(Self::Other(buf.value().to_vec()))
    }
}
