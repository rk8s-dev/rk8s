// SPDX-License-Identifier: MIT

use std::fmt::Debug;

use anyhow::Context;
use netlink_packet_utils::DecodeError;

use crate::{
    payload::{NLMSG_DONE, NLMSG_ERROR, NLMSG_NOOP, NLMSG_OVERRUN},
    DoneBuffer, DoneMessage, Emitable, ErrorBuffer, ErrorMessage,
    NetlinkBuffer, NetlinkDeserializable, NetlinkHeader, NetlinkPayload,
    NetlinkSerializable, Parseable,
};

/// Represent a netlink message.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct NetlinkMessage<I> {
    /// Message header (this is common to all the netlink protocols)
    pub header: NetlinkHeader,
    /// Inner message, which depends on the netlink protocol being used.
    pub payload: NetlinkPayload<I>,
}

impl<I> NetlinkMessage<I> {
    /// Create a new netlink message from the given header and payload
    pub fn new(header: NetlinkHeader, payload: NetlinkPayload<I>) -> Self {
        NetlinkMessage { header, payload }
    }

    /// Consume this message and return its header and payload
    pub fn into_parts(self) -> (NetlinkHeader, NetlinkPayload<I>) {
        (self.header, self.payload)
    }
}

impl<I> NetlinkMessage<I>
where
    I: NetlinkDeserializable,
{
    /// Parse the given buffer as a netlink message
    pub fn deserialize(buffer: &[u8]) -> Result<Self, DecodeError> {
        let netlink_buffer = NetlinkBuffer::new_checked(&buffer)?;
        <Self as Parseable<NetlinkBuffer<&&[u8]>>>::parse(&netlink_buffer)
    }
}

impl<I> NetlinkMessage<I>
where
    I: NetlinkSerializable,
{
    /// Return the length of this message in bytes
    pub fn buffer_len(&self) -> usize {
        <Self as Emitable>::buffer_len(self)
    }

    /// Serialize this message and write the serialized data into the
    /// given buffer. `buffer` must big large enough for the whole
    /// message to fit, otherwise, this method will panic. To know how
    /// big the serialized message is, call `buffer_len()`.
    ///
    /// # Panic
    ///
    /// This method panics if the buffer is not big enough.
    pub fn serialize(&self, buffer: &mut [u8]) {
        self.emit(buffer)
    }

    /// Ensure the header (`NetlinkHeader`) is consistent with the payload
    /// (`NetlinkPayload`):
    ///
    /// - compute the payload length and set the header's length field
    /// - check the payload type and set the header's message type field
    ///   accordingly
    ///
    /// If you are not 100% sure the header is correct, this method should be
    /// called before calling [`Emitable::emit()`](trait.Emitable.html#
    /// tymethod.emit), as it could panic if the header is inconsistent with
    /// the rest of the message.
    pub fn finalize(&mut self) {
        self.header.length = self.buffer_len() as u32;
        self.header.message_type = self.payload.message_type();
    }
}

impl<'buffer, B, I> Parseable<NetlinkBuffer<&'buffer B>> for NetlinkMessage<I>
where
    B: AsRef<[u8]> + 'buffer,
    I: NetlinkDeserializable,
{
    fn parse(buf: &NetlinkBuffer<&'buffer B>) -> Result<Self, DecodeError> {
        use self::NetlinkPayload::*;

        let header =
            <NetlinkHeader as Parseable<NetlinkBuffer<&'buffer B>>>::parse(buf)
                .context("failed to parse netlink header")?;

        let bytes = buf.payload();
        let payload = match header.message_type {
            NLMSG_ERROR => {
                let msg = ErrorBuffer::new_checked(&bytes)
                    .and_then(|buf| ErrorMessage::parse(&buf))
                    .context("failed to parse NLMSG_ERROR")?;
                Error(msg)
            }
            NLMSG_NOOP => Noop,
            NLMSG_DONE => {
                let msg = DoneBuffer::new_checked(&bytes)
                    .and_then(|buf| DoneMessage::parse(&buf))
                    .context("failed to parse NLMSG_DONE")?;
                Done(msg)
            }
            NLMSG_OVERRUN => Overrun(bytes.to_vec()),
            message_type => {
                let inner_msg = I::deserialize(&header, bytes).context(
                    format!("Failed to parse message with type {message_type}"),
                )?;
                InnerMessage(inner_msg)
            }
        };
        Ok(NetlinkMessage { header, payload })
    }
}

impl<I> Emitable for NetlinkMessage<I>
where
    I: NetlinkSerializable,
{
    fn buffer_len(&self) -> usize {
        use self::NetlinkPayload::*;

        let payload_len = match self.payload {
            Noop => 0,
            Done(ref msg) => msg.buffer_len(),
            Overrun(ref bytes) => bytes.len(),
            Error(ref msg) => msg.buffer_len(),
            InnerMessage(ref msg) => msg.buffer_len(),
        };

        self.header.buffer_len() + payload_len
    }

    fn emit(&self, buffer: &mut [u8]) {
        use self::NetlinkPayload::*;

        self.header.emit(buffer);

        let buffer =
            &mut buffer[self.header.buffer_len()..self.header.length as usize];
        match self.payload {
            Noop => {}
            Done(ref msg) => msg.emit(buffer),
            Overrun(ref bytes) => buffer.copy_from_slice(bytes),
            Error(ref msg) => msg.emit(buffer),
            InnerMessage(ref msg) => msg.serialize(buffer),
        }
    }
}

impl<T> From<T> for NetlinkMessage<T>
where
    T: Into<NetlinkPayload<T>>,
{
    fn from(inner_message: T) -> Self {
        NetlinkMessage {
            header: NetlinkHeader::default(),
            payload: inner_message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{convert::Infallible, mem::size_of, num::NonZeroI32};

    #[derive(Clone, Debug, Default, PartialEq)]
    struct FakeNetlinkInnerMessage;

    impl NetlinkSerializable for FakeNetlinkInnerMessage {
        fn message_type(&self) -> u16 {
            unimplemented!("unused by tests")
        }

        fn buffer_len(&self) -> usize {
            unimplemented!("unused by tests")
        }

        fn serialize(&self, _buffer: &mut [u8]) {
            unimplemented!("unused by tests")
        }
    }

    impl NetlinkDeserializable for FakeNetlinkInnerMessage {
        type Error = Infallible;

        fn deserialize(
            _header: &NetlinkHeader,
            _payload: &[u8],
        ) -> Result<Self, Self::Error> {
            unimplemented!("unused by tests")
        }
    }

    #[test]
    fn test_done() {
        let header = NetlinkHeader::default();
        let done_msg = DoneMessage {
            code: 0,
            extended_ack: vec![6, 7, 8, 9],
        };
        let mut want = NetlinkMessage::new(
            header,
            NetlinkPayload::<FakeNetlinkInnerMessage>::Done(done_msg.clone()),
        );
        want.finalize();

        let len = want.buffer_len();
        assert_eq!(
            len,
            header.buffer_len()
                + size_of::<i32>()
                + done_msg.extended_ack.len()
        );

        let mut buf = vec![1; len];
        want.emit(&mut buf);

        let done_buf = DoneBuffer::new(&buf[header.buffer_len()..]);
        assert_eq!(done_buf.code(), done_msg.code);
        assert_eq!(done_buf.extended_ack(), &done_msg.extended_ack);

        let got = NetlinkMessage::parse(&NetlinkBuffer::new(&buf)).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn test_error() {
        // SAFETY: value is non-zero.
        const ERROR_CODE: NonZeroI32 =
            unsafe { NonZeroI32::new_unchecked(-8765) };

        let header = NetlinkHeader::default();
        let error_msg = ErrorMessage {
            code: Some(ERROR_CODE),
            header: vec![],
        };
        let mut want = NetlinkMessage::new(
            header,
            NetlinkPayload::<FakeNetlinkInnerMessage>::Error(error_msg.clone()),
        );
        want.finalize();

        let len = want.buffer_len();
        assert_eq!(len, header.buffer_len() + error_msg.buffer_len());

        let mut buf = vec![1; len];
        want.emit(&mut buf);

        let error_buf = ErrorBuffer::new(&buf[header.buffer_len()..]);
        assert_eq!(error_buf.code(), error_msg.code);

        let got = NetlinkMessage::parse(&NetlinkBuffer::new(&buf)).unwrap();
        assert_eq!(got, want);
    }
}
