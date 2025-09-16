// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable};

const RULE_REALM_LEN: usize = 4;

#[derive(Clone, Eq, PartialEq, Debug, Copy)]
pub struct RouteRealm {
    pub source: u16,
    pub destination: u16,
}

impl RouteRealm {
    pub(crate) fn parse(buf: &[u8]) -> Result<Self, DecodeError> {
        let all = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if buf.len() == RULE_REALM_LEN {
            Ok(Self {
                source: (all >> 16) as u16,
                destination: (all & 0xFFFF) as u16,
            })
        } else {
            Err(DecodeError::from(format!(
                "Invalid rule port range data, expecting \
                {RULE_REALM_LEN} u8 array, but got {:?}",
                buf
            )))
        }
    }
}

impl Emitable for RouteRealm {
    fn buffer_len(&self) -> usize {
        RULE_REALM_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let all = ((self.source as u32) << 16) | self.destination as u32;
        buffer.copy_from_slice(&all.to_ne_bytes());
    }
}
