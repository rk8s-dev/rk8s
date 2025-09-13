// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable};

const RULE_PORT_RANGE_LEN: usize = 4;

#[derive(Clone, Eq, PartialEq, Debug, Copy)]
pub struct RulePortRange {
    pub start: u16,
    pub end: u16,
}

impl RulePortRange {
    pub(crate) fn parse(buf: &[u8]) -> Result<Self, DecodeError> {
        if buf.len() == RULE_PORT_RANGE_LEN {
            Ok(Self {
                start: u16::from_ne_bytes([buf[0], buf[1]]),
                end: u16::from_ne_bytes([buf[2], buf[3]]),
            })
        } else {
            Err(DecodeError::from(format!(
                "Invalid rule port range data, expecting \
                {RULE_PORT_RANGE_LEN} u8 array, but got {:?}",
                buf
            )))
        }
    }
}

impl Emitable for RulePortRange {
    fn buffer_len(&self) -> usize {
        RULE_PORT_RANGE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer[0..2].copy_from_slice(&self.start.to_ne_bytes());
        buffer[2..4].copy_from_slice(&self.end.to_ne_bytes());
    }
}
