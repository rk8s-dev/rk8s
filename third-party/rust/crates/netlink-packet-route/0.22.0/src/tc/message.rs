// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use super::{TcAttribute, TcHeader, TcMessageBuffer};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct TcMessage {
    pub header: TcHeader,
    pub attributes: Vec<TcAttribute>,
}

impl TcMessage {
    pub fn into_parts(self) -> (TcHeader, Vec<TcAttribute>) {
        (self.header, self.attributes)
    }

    pub fn from_parts(header: TcHeader, attributes: Vec<TcAttribute>) -> Self {
        TcMessage { header, attributes }
    }

    /// Create a new `TcMessage` with the given index
    pub fn with_index(index: i32) -> Self {
        Self {
            header: TcHeader {
                index,
                ..Default::default()
            },
            attributes: Vec::new(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<TcMessageBuffer<&'a T>> for TcMessage {
    fn parse(buf: &TcMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: TcHeader::parse(buf)
                .context("failed to parse tc message header")?,
            attributes: Vec::<TcAttribute>::parse(buf)
                .context("failed to parse tc message NLAs")?,
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<TcMessageBuffer<&'a T>>
    for Vec<TcAttribute>
{
    fn parse(buf: &TcMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        let mut kind = String::new();

        for nla_buf in buf.attributes() {
            let attribute =
                TcAttribute::parse_with_param(&nla_buf?, kind.as_str())?;
            if let TcAttribute::Kind(s) = &attribute {
                kind = s.to_string();
            }
            attributes.push(attribute)
        }
        Ok(attributes)
    }
}

impl Emitable for TcMessage {
    fn buffer_len(&self) -> usize {
        self.header.buffer_len() + self.attributes.as_slice().buffer_len()
    }

    fn emit(&self, buffer: &mut [u8]) {
        self.header.emit(buffer);
        self.attributes
            .as_slice()
            .emit(&mut buffer[self.header.buffer_len()..]);
    }
}
