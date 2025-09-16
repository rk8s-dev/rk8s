// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{super::AddressFamily, flags::RuleFlags, RuleAction};

const RULE_HEADER_LEN: usize = 12;

buffer!(RuleMessageBuffer(RULE_HEADER_LEN) {
    family: (u8, 0),
    dst_len: (u8, 1),
    src_len: (u8, 2),
    tos: (u8, 3),
    table: (u8, 4),
    reserve_1: (u8, 5),
    reserve_2: (u8, 6),
    action: (u8, 7),
    flags: (u32, 8..RULE_HEADER_LEN),
    payload: (slice, RULE_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> RuleMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

// Linux kernel code `struct fib_rule_hdr`
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct RuleHeader {
    pub family: AddressFamily,
    pub dst_len: u8,
    pub src_len: u8,
    pub tos: u8,
    pub table: u8,
    pub action: RuleAction,
    pub flags: RuleFlags,
}

impl Emitable for RuleHeader {
    fn buffer_len(&self) -> usize {
        RULE_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = RuleMessageBuffer::new(buffer);
        packet.set_family(self.family.into());
        packet.set_dst_len(self.dst_len);
        packet.set_src_len(self.src_len);
        packet.set_table(self.table);
        packet.set_tos(self.tos);
        packet.set_action(self.action.into());
        packet.set_flags(self.flags.bits());
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<RuleMessageBuffer<&'a T>>
    for RuleHeader
{
    fn parse(buf: &RuleMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(RuleHeader {
            family: buf.family().into(),
            dst_len: buf.dst_len(),
            src_len: buf.src_len(),
            tos: buf.tos(),
            table: buf.table(),
            action: buf.action().into(),
            flags: RuleFlags::from_bits_retain(buf.flags()),
        })
    }
}
