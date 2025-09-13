// SPDX-License-Identifier: MIT

/// U32 filter
///
/// In its simplest form the U32 filter is a list of records, each
/// consisting of two fields: a selector and an action. The selectors,
/// described below, are compared with the currently processed IP packet
/// until the first match occurs, and then the associated action is
/// performed.
use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::parse_u32,
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::u32_flags::{TcU32OptionFlags, TcU32SelectorFlags};
use crate::tc::{TcAction, TcHandle};

const TC_U32_SEL_BUF_LEN: usize = 16;
const TC_U32_KEY_BUF_LEN: usize = 16;

const TCA_U32_CLASSID: u16 = 1;
const TCA_U32_HASH: u16 = 2;
const TCA_U32_LINK: u16 = 3;
const TCA_U32_DIVISOR: u16 = 4;
const TCA_U32_SEL: u16 = 5;
const TCA_U32_POLICE: u16 = 6;
const TCA_U32_ACT: u16 = 7;
const TCA_U32_INDEV: u16 = 8;
const TCA_U32_PCNT: u16 = 9;
const TCA_U32_MARK: u16 = 10;
const TCA_U32_FLAGS: u16 = 11;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcFilterU32 {}

impl TcFilterU32 {
    pub const KIND: &'static str = "u32";
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcFilterU32Option {
    ClassId(TcHandle),
    Hash(u32),
    Link(u32),
    Divisor(u32),
    Selector(TcU32Selector),
    Police(Vec<u8>),
    Action(Vec<TcAction>),
    Indev(Vec<u8>),
    Pnct(Vec<u8>),
    Mark(Vec<u8>),
    Flags(TcU32OptionFlags),
    Other(DefaultNla),
}

impl Nla for TcFilterU32Option {
    fn value_len(&self) -> usize {
        match self {
            Self::Police(b)
            | Self::Indev(b)
            | Self::Pnct(b)
            | Self::Mark(b) => b.len(),
            Self::Hash(_)
            | Self::Link(_)
            | Self::Divisor(_)
            | Self::Flags(_) => 4,
            Self::ClassId(_) => 4,
            Self::Selector(s) => s.buffer_len(),
            Self::Action(acts) => acts.as_slice().buffer_len(),
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Police(b)
            | Self::Indev(b)
            | Self::Pnct(b)
            | Self::Mark(b) => buffer.copy_from_slice(b.as_slice()),
            Self::Hash(i) | Self::Link(i) | Self::Divisor(i) => {
                NativeEndian::write_u32(buffer, *i)
            }
            Self::Flags(f) => NativeEndian::write_u32(buffer, f.bits()),
            Self::ClassId(i) => NativeEndian::write_u32(buffer, (*i).into()),
            Self::Selector(s) => s.emit(buffer),
            Self::Action(acts) => acts.as_slice().emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::ClassId(_) => TCA_U32_CLASSID,
            Self::Hash(_) => TCA_U32_HASH,
            Self::Link(_) => TCA_U32_LINK,
            Self::Divisor(_) => TCA_U32_DIVISOR,
            Self::Selector(_) => TCA_U32_SEL,
            Self::Police(_) => TCA_U32_POLICE,
            Self::Action(_) => TCA_U32_ACT,
            Self::Indev(_) => TCA_U32_INDEV,
            Self::Pnct(_) => TCA_U32_PCNT,
            Self::Mark(_) => TCA_U32_MARK,
            Self::Flags(_) => TCA_U32_FLAGS,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcFilterU32Option
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_U32_CLASSID => Self::ClassId(TcHandle::from(
                parse_u32(payload).context("failed to parse TCA_U32_UNSPEC")?,
            )),
            TCA_U32_HASH => Self::Hash(
                parse_u32(payload).context("failed to parse TCA_U32_HASH")?,
            ),
            TCA_U32_LINK => Self::Link(
                parse_u32(payload).context("failed to parse TCA_U32_LINK")?,
            ),
            TCA_U32_DIVISOR => Self::Divisor(
                parse_u32(payload)
                    .context("failed to parse TCA_U32_DIVISOR")?,
            ),
            TCA_U32_SEL => Self::Selector(
                TcU32Selector::parse(
                    &TcU32SelectorBuffer::new_checked(payload)
                        .context("invalid TCA_U32_SEL")?,
                )
                .context("failed to parse TCA_U32_SEL")?,
            ),
            TCA_U32_POLICE => Self::Police(payload.to_vec()),
            TCA_U32_ACT => {
                let mut acts = vec![];
                for act in NlasIterator::new(payload) {
                    let act = act.context("invalid TCA_U32_ACT")?;
                    acts.push(
                        TcAction::parse(&act)
                            .context("failed to parse TCA_U32_ACT")?,
                    );
                }
                Self::Action(acts)
            }
            TCA_U32_INDEV => Self::Indev(payload.to_vec()),
            TCA_U32_PCNT => Self::Pnct(payload.to_vec()),
            TCA_U32_MARK => Self::Mark(payload.to_vec()),
            TCA_U32_FLAGS => Self::Flags(TcU32OptionFlags::from_bits_retain(
                parse_u32(payload).context("failed to parse TCA_U32_FLAGS")?,
            )),
            _ => Self::Other(
                DefaultNla::parse(buf).context("failed to parse u32 nla")?,
            ),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct TcU32Selector {
    pub flags: TcU32SelectorFlags,
    pub offshift: u8,
    pub nkeys: u8,
    pub offmask: u16,
    pub off: u16,
    pub offoff: u16,
    pub hoff: u16,
    pub hmask: u32,
    pub keys: Vec<TcU32Key>,
}

buffer!(TcU32SelectorBuffer {
    flags: (u8, 0),
    offshift: (u8, 1),
    nkeys: (u8, 2),
    //pad: (u8, 3),
    offmask: (u16, 4..6),
    off: (u16, 6..8),
    offoff: (u16, 8..10),
    hoff: (u16, 10..12),
    hmask: (u32, 12..TC_U32_SEL_BUF_LEN),
    keys: (slice, TC_U32_SEL_BUF_LEN..),
});

impl<T: AsRef<[u8]>> TcU32SelectorBuffer<T> {
    pub fn new_checked(buffer: T) -> Result<Self, DecodeError> {
        let packet = Self::new(buffer);
        packet.check_buffer_length()?;
        Ok(packet)
    }

    fn check_buffer_length(&self) -> Result<(), DecodeError> {
        let len = self.buffer.as_ref().len();
        if len < TC_U32_SEL_BUF_LEN {
            return Err(format!(
                "invalid TcU32SelectorBuffer: length {len} < {TC_U32_SEL_BUF_LEN}"
            )
            .into());
        }
        // Expect the buffer to be large enough to hold `nkeys`.
        let expected_len =
            ((self.nkeys() as usize) * TC_U32_KEY_BUF_LEN) + TC_U32_SEL_BUF_LEN;
        if len < expected_len {
            return Err(format!(
                "invalid RouteNextHopBuffer: length {} < {}",
                len, expected_len,
            )
            .into());
        }
        Ok(())
    }
}

impl Emitable for TcU32Selector {
    fn buffer_len(&self) -> usize {
        TC_U32_SEL_BUF_LEN + (self.nkeys as usize * TC_U32_KEY_BUF_LEN)
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcU32SelectorBuffer::new(buffer);
        packet.set_flags(self.flags.bits());
        packet.set_offshift(self.offshift);
        packet.set_offmask(self.offmask);
        packet.set_off(self.off);
        packet.set_offoff(self.offoff);
        packet.set_hoff(self.hoff);
        packet.set_hmask(self.hmask);
        packet.set_nkeys(self.nkeys);

        let key_buf = packet.keys_mut();
        for (i, k) in self.keys.iter().enumerate() {
            k.emit(
                &mut key_buf
                    [(i * TC_U32_KEY_BUF_LEN)..((i + 1) * TC_U32_KEY_BUF_LEN)],
            );
        }
    }
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<TcU32SelectorBuffer<&T>>
    for TcU32Selector
{
    fn parse(buf: &TcU32SelectorBuffer<&T>) -> Result<Self, DecodeError> {
        let nkeys = buf.nkeys();
        let mut keys = Vec::<TcU32Key>::with_capacity(nkeys.into());
        let key_payload = buf.keys();
        for i in 0..nkeys {
            let i = i as usize;
            let keybuf = TcU32KeyBuffer::new_checked(
                &key_payload
                    [(i * TC_U32_KEY_BUF_LEN)..(i + 1) * TC_U32_KEY_BUF_LEN],
            )
            .context("invalid u32 key")?;
            keys.push(
                TcU32Key::parse(&keybuf).context("failed to parse u32 key")?,
            );
        }

        Ok(Self {
            flags: TcU32SelectorFlags::from_bits_retain(buf.flags()),
            offshift: buf.offshift(),
            nkeys,
            offmask: buf.offmask(),
            off: buf.off(),
            offoff: buf.offoff(),
            hoff: buf.hoff(),
            hmask: buf.hmask(),
            keys,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct TcU32Key {
    pub mask: u32,
    pub val: u32,
    pub off: i32,
    pub offmask: i32,
}

buffer!(TcU32KeyBuffer(TC_U32_KEY_BUF_LEN) {
    mask: (u32, 0..4),
    val: (u32, 4..8),
    off: (i32, 8..12),
    offmask: (i32, 12..TC_U32_KEY_BUF_LEN),
});

impl Emitable for TcU32Key {
    fn buffer_len(&self) -> usize {
        TC_U32_KEY_BUF_LEN
    }
    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcU32KeyBuffer::new(buffer);
        packet.set_mask(self.mask);
        packet.set_val(self.val);
        packet.set_off(self.off);
        packet.set_offmask(self.offmask);
    }
}

impl<T: AsRef<[u8]>> Parseable<TcU32KeyBuffer<T>> for TcU32Key {
    fn parse(buf: &TcU32KeyBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            mask: buf.mask(),
            val: buf.val(),
            off: buf.off(),
            offmask: buf.offmask(),
        })
    }
}
