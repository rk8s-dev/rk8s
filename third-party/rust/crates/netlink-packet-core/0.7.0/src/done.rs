// SPDX-License-Identifier: MIT

use std::mem::size_of;

use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::DecodeError;

use crate::{Emitable, Field, Parseable, Rest};

const CODE: Field = 0..4;
const EXTENDED_ACK: Rest = 4..;
const DONE_HEADER_LEN: usize = EXTENDED_ACK.start;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct DoneBuffer<T> {
    buffer: T,
}

impl<T: AsRef<[u8]>> DoneBuffer<T> {
    pub fn new(buffer: T) -> DoneBuffer<T> {
        DoneBuffer { buffer }
    }

    /// Consume the packet, returning the underlying buffer.
    pub fn into_inner(self) -> T {
        self.buffer
    }

    pub fn new_checked(buffer: T) -> Result<Self, DecodeError> {
        let packet = Self::new(buffer);
        packet.check_buffer_length()?;
        Ok(packet)
    }

    fn check_buffer_length(&self) -> Result<(), DecodeError> {
        let len = self.buffer.as_ref().len();
        if len < DONE_HEADER_LEN {
            Err(format!(
                "invalid DoneBuffer: length is {len} but DoneBuffer are \
                at least {DONE_HEADER_LEN} bytes"
            )
            .into())
        } else {
            Ok(())
        }
    }

    /// Return the error code
    pub fn code(&self) -> i32 {
        let data = self.buffer.as_ref();
        NativeEndian::read_i32(&data[CODE])
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> DoneBuffer<&'a T> {
    /// Return a pointer to the extended ack attributes.
    pub fn extended_ack(&self) -> &'a [u8] {
        let data = self.buffer.as_ref();
        &data[EXTENDED_ACK]
    }
}

impl<'a, T: AsRef<[u8]> + AsMut<[u8]> + ?Sized> DoneBuffer<&'a mut T> {
    /// Return a mutable pointer to the extended ack attributes.
    pub fn extended_ack_mut(&mut self) -> &mut [u8] {
        let data = self.buffer.as_mut();
        &mut data[EXTENDED_ACK]
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> DoneBuffer<T> {
    /// set the error code field
    pub fn set_code(&mut self, value: i32) {
        let data = self.buffer.as_mut();
        NativeEndian::write_i32(&mut data[CODE], value)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DoneMessage {
    pub code: i32,
    pub extended_ack: Vec<u8>,
}

impl Emitable for DoneMessage {
    fn buffer_len(&self) -> usize {
        size_of::<i32>() + self.extended_ack.len()
    }
    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = DoneBuffer::new(buffer);
        buffer.set_code(self.code);
        buffer
            .extended_ack_mut()
            .copy_from_slice(&self.extended_ack);
    }
}

impl<'buffer, T: AsRef<[u8]> + 'buffer> Parseable<DoneBuffer<&'buffer T>>
    for DoneMessage
{
    fn parse(buf: &DoneBuffer<&'buffer T>) -> Result<DoneMessage, DecodeError> {
        Ok(DoneMessage {
            code: buf.code(),
            extended_ack: buf.extended_ack().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_and_parse() {
        let expected = DoneMessage {
            code: 5,
            extended_ack: vec![1, 2, 3],
        };

        let len = expected.buffer_len();
        assert_eq!(len, size_of::<i32>() + expected.extended_ack.len());

        let mut buf = vec![0; len];
        expected.emit(&mut buf);

        let done_buf = DoneBuffer::new(&buf);
        assert_eq!(done_buf.code(), expected.code);
        assert_eq!(done_buf.extended_ack(), &expected.extended_ack);

        let got = DoneMessage::parse(&done_buf).unwrap();
        assert_eq!(got, expected);
    }
}
