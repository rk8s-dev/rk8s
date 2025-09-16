// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable};

const RULE_UID_RANGE_LEN: usize = 8;

#[derive(Clone, Eq, PartialEq, Debug, Copy)]
pub struct RuleUidRange {
    pub start: u32,
    pub end: u32,
}

impl RuleUidRange {
    pub(crate) fn parse(buf: &[u8]) -> Result<Self, DecodeError> {
        if buf.len() == RULE_UID_RANGE_LEN {
            Ok(Self {
                start: u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]),
                end: u32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]),
            })
        } else {
            Err(DecodeError::from(format!(
                "Invalid rule port range data, expecting \
                {RULE_UID_RANGE_LEN} u8 array, but got {:?}",
                buf
            )))
        }
    }
}

impl Emitable for RuleUidRange {
    fn buffer_len(&self) -> usize {
        RULE_UID_RANGE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer[0..4].copy_from_slice(&self.start.to_ne_bytes());
        buffer[4..8].copy_from_slice(&self.end.to_ne_bytes());
    }
}
