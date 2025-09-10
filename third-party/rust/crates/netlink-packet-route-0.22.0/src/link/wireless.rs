// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

// Place holder for kernel code is `struct iw_event`
#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct LinkWirelessEvent(Vec<u8>);

impl<T: AsRef<[u8]> + ?Sized> Parseable<T> for LinkWirelessEvent {
    fn parse(buf: &T) -> Result<Self, DecodeError> {
        Ok(LinkWirelessEvent(buf.as_ref().to_vec()))
    }
}

impl Emitable for LinkWirelessEvent {
    fn buffer_len(&self) -> usize {
        self.0.len()
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer.copy_from_slice(self.0.as_slice())
    }
}
