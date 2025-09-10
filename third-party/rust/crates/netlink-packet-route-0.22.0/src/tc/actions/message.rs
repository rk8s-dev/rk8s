// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::nla::{DefaultNla, NlaBuffer};
use netlink_packet_utils::nla::{Nla, NlasIterator};
use netlink_packet_utils::{DecodeError, Emitable, Parseable};

use crate::tc::actions::{TcActionMessageBuffer, TcActionMessageHeader};
use crate::tc::TcAction;

/// Message to describe [tc-actions]
///
/// [tc-actions]: https://man7.org/linux/man-pages/man8/tc-actions.8.html
#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct TcActionMessage {
    /// Header of the message.
    pub header: TcActionMessageHeader,
    /// Attributes of the message.
    pub attributes: Vec<TcActionMessageAttribute>,
}

const TCA_ACT_FLAG_LARGE_DUMP_ON: u32 = 1 << 0;
const TCA_ACT_FLAG_TERSE_DUMP: u32 = 1 << 1;

bitflags! {
    /// Flags to configure action dumps (list operations).
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Default, PartialOrd, Ord, Hash)]
    #[non_exhaustive]
    pub struct TcActionMessageFlags: u32 {
        /// If set, this flag enables more than `TCA_ACT_MAX_PRIO` actions in a single
        /// actions listing operation.
        const LargeDump = TCA_ACT_FLAG_LARGE_DUMP_ON;
        /// If set, this flag restricts an action dump to only include essential
        /// details.
        const TerseDump = TCA_ACT_FLAG_TERSE_DUMP;
        const _ = !0;
    }
}

/// [`TcActionMessageFlagsWithSelector`] sets the [`TcActionMessageFlags`] which
/// are to be included in an operation, based on the accompanying [`flags`] and
/// [`selector`] fields.
///
/// [`flags`]: #structfield.flags
/// [`selector`]: #structfield.selector
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, PartialOrd, Ord, Hash)]
pub struct TcActionMessageFlagsWithSelector {
    /// A bitmask of [`TcActionMessageFlags`] to be associated with an
    /// operation.
    pub flags: TcActionMessageFlags,
    /// A bitmask to determine which flags are to be included in an operation.
    ///
    /// Any flags which are set in the [`flags`] field but which are not set in
    /// the [`selector`] field will be ignored.
    ///
    /// [`flags`]: #structfield.flags
    /// [`selector`]: #structfield.selector
    pub selector: TcActionMessageFlags,
}

impl Nla for TcActionMessageFlagsWithSelector {
    fn value_len(&self) -> usize {
        8
    }

    fn kind(&self) -> u16 {
        TCA_ROOT_FLAGS
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        buffer[..4].copy_from_slice(&self.flags.bits().to_ne_bytes());
        buffer[4..8].copy_from_slice(&self.selector.bits().to_ne_bytes());
    }
}

impl TcActionMessageFlagsWithSelector {
    /// Create a new [`TcActionMessageFlagsWithSelector`] with the given
    /// [`flags`].
    /// The [`selector`] field is set to the same value as [`flags`] (i.e., none
    /// of the [`flags`] will be ignored).
    ///
    /// [`flags`]: #structfield.flags
    /// [`selector`]: #structfield.selector
    #[must_use]
    pub fn new(flags: TcActionMessageFlags) -> Self {
        Self {
            flags,
            selector: flags,
        }
    }

    /// Create a new [`TcActionMessageFlagsWithSelector`] with the given
    /// [`flags`] and [`selector`].
    ///
    /// [`flags`]: #structfield.flags
    /// [`selector`]: #structfield.selector
    #[must_use]
    pub fn new_with_selector(
        flags: TcActionMessageFlags,
        selector: TcActionMessageFlags,
    ) -> Self {
        Self { flags, selector }
    }
}

impl<'a, T: AsRef<[u8]> + 'a + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcActionMessageFlagsWithSelector
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let value = buf.value();
        if value.len() != 8 {
            return Err(DecodeError::from("invalid length"));
        }
        let flags = TcActionMessageFlags::from_bits(u32::from_ne_bytes(
            value[0..4].try_into().context("invalid length")?,
        ))
        .ok_or_else(|| DecodeError::from("invalid flags"))?;
        let selector = TcActionMessageFlags::from_bits(u32::from_ne_bytes(
            value[4..].try_into().context("invalid length")?,
        ))
        .ok_or_else(|| DecodeError::from("invalid flags selector"))?;
        Ok(Self::new_with_selector(flags, selector))
    }
}

const TCA_ACT_TAB: u16 = 1;
const TCA_ROOT_FLAGS: u16 = 2;
const TCA_ROOT_COUNT: u16 = 3;
const TCA_ROOT_TIME_DELTA: u16 = 4;
const TCA_ROOT_EXT_WARN_MSG: u16 = 5;

/// This enum is used to represent the different types of attributes that can be
/// part of a [`TcActionMessage`].
///
/// This enum is non-exhaustive, additional variants may be added in the future.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionMessageAttribute {
    /// Collection of `TcActions`.
    Actions(Vec<TcAction>),
    /// Flags to configure action dumps (list operations).
    Flags(TcActionMessageFlagsWithSelector),
    /// Number of actions being dumped.
    RootCount(u32),
    /// Time delta.
    RootTimeDelta(u32),
    /// Extended warning message.
    RootExtWarnMsg(String),
    /// Other attributes unknown at the time of writing.
    Other(DefaultNla),
}

impl<'a, T: AsRef<[u8]> + 'a + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcActionMessageAttribute
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(match buf.kind() {
            TCA_ACT_TAB => {
                let actions = NlasIterator::new(buf.value())
                    .map(|nla| TcAction::parse(&nla?))
                    .collect::<Result<Vec<_>, _>>()?;
                Self::Actions(actions)
            }
            TCA_ROOT_FLAGS => {
                Self::Flags(TcActionMessageFlagsWithSelector::parse(buf)?)
            }
            TCA_ROOT_COUNT => {
                let count = u32::from_ne_bytes(
                    buf.value().try_into().context("invalid length")?,
                );
                Self::RootCount(count)
            }
            TCA_ROOT_TIME_DELTA => {
                let delta = u32::from_be_bytes(
                    buf.value().try_into().context("invalid length")?,
                );
                Self::RootTimeDelta(delta)
            }
            TCA_ROOT_EXT_WARN_MSG => {
                let msg = String::from_utf8(buf.value().to_vec())
                    .context("invalid utf8")?;
                Self::RootExtWarnMsg(msg)
            }
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

impl Nla for TcActionMessageAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Actions(actions) => actions.as_slice().buffer_len(),
            Self::Flags(_) => 8,
            Self::RootCount(_) => 4,
            Self::RootTimeDelta(_) => 4,
            Self::RootExtWarnMsg(msg) => msg.len(),
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Actions(_) => TCA_ACT_TAB,
            Self::Flags(_) => TCA_ROOT_FLAGS,
            Self::RootCount(_) => TCA_ROOT_COUNT,
            Self::RootTimeDelta(_) => TCA_ROOT_TIME_DELTA,
            Self::RootExtWarnMsg(_) => TCA_ROOT_EXT_WARN_MSG,
            Self::Other(nla) => nla.kind(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Actions(actions) => actions.as_slice().emit(buffer),
            Self::Flags(flags) => {
                flags.emit_value(buffer);
            }
            Self::RootCount(count) => {
                buffer.copy_from_slice(&count.to_ne_bytes());
            }
            Self::RootTimeDelta(delta) => {
                buffer.copy_from_slice(&delta.to_be_bytes());
            }
            Self::RootExtWarnMsg(msg) => buffer.copy_from_slice(msg.as_bytes()),
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }
}

impl<'a, T: AsRef<[u8]> + 'a + ?Sized> Parseable<TcActionMessageBuffer<&'a T>>
    for TcActionMessage
{
    fn parse(buf: &TcActionMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let attrs: Result<Vec<_>, DecodeError> = buf
            .attributes()
            .map(|attr| TcActionMessageAttribute::parse(&attr?))
            .collect::<Result<Vec<_>, _>>();

        Ok(Self {
            header: TcActionMessageHeader::parse(buf)
                .context("failed to parse tc message header")?,
            attributes: attrs?,
        })
    }
}

impl Emitable for TcActionMessage {
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
