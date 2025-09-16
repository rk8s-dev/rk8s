// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::nla::NLA_F_NESTED;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_string, parse_u32},
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use crate::tc::TcStats2;

use super::{
    TcActionMirror, TcActionMirrorOption, TcActionNat, TcActionNatOption,
    TcActionTunnelKey, TcActionTunnelKeyOption,
};

/// TODO: determine when and why to use this as opposed to the buffer's `kind`.
const TCA_ACT_TAB: u16 = 1;

/// [`TcAction`] is a netlink message attribute that describes a [tc-action].
///
/// [tc-action]: https://man7.org/linux/man-pages/man8/tc-actions.8.html
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcAction {
    /// Table id.
    /// Corresponds to the [`Kind`] of the action.
    ///
    /// [`Kind`]: crate::tc::TcActionAttribute::Kind
    pub tab: u16,
    /// Attributes of the action.
    pub attributes: Vec<TcActionAttribute>,
}

impl Default for TcAction {
    fn default() -> Self {
        Self {
            tab: TCA_ACT_TAB,
            attributes: Vec::new(),
        }
    }
}

impl Nla for TcAction {
    fn value_len(&self) -> usize {
        self.attributes.as_slice().buffer_len()
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        self.attributes.as_slice().emit(buffer);
    }

    fn kind(&self) -> u16 {
        self.tab
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for TcAction {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        // We need to find the `Kind` attribute before we can parse the others,
        // as kind is used in calls to parse_with_param for the other
        // attributes.
        // Messages of this type which do not specify [`Kind`], or which specify
        // `Kind` more than once are malformed and should be rejected.
        // We cannot ensure that `Kind` will be the first attribute in the
        // `attributes` `Vec` (although it usually is).
        // As a result, we need to determine `Kind` first, then parse the rest
        // of the attributes.
        let kind = match NlasIterator::new(buf.value())
            .filter_map(|nla| {
                let nla = match nla {
                    Ok(nla) => nla,
                    Err(e) => {
                        return Some(
                            Err(e).context("failed to parse action nla"),
                        )
                    }
                };
                match nla.kind() {
                    TCA_ACT_KIND => Some(
                        parse_string(nla.value())
                            .context("failed to parse TCA_ACT_KIND"),
                    ),
                    _ => None,
                }
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(kinds) => {
                if kinds.is_empty() {
                    return Err(DecodeError::from("Missing TCA_ACT_KIND"));
                }
                if kinds.len() > 1 {
                    return Err(DecodeError::from("Duplicate TCA_ACT_KIND"));
                }
                kinds[0].clone()
            }
            Err(e) => return Err(DecodeError::from(e.to_string())),
        };

        let attributes = NlasIterator::new(buf.value())
            .map(|nla| {
                TcActionAttribute::parse_with_param(&nla?, kind.as_str())
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            tab: buf.kind(),
            attributes,
        })
    }
}

const TCA_ACT_KIND: u16 = 1;
const TCA_ACT_OPTIONS: u16 = 2;
const TCA_ACT_INDEX: u16 = 3;
const TCA_ACT_STATS: u16 = 4;
// const TCA_ACT_PAD: u16 = 5;
const TCA_ACT_COOKIE: u16 = 6;
// const TCA_ACT_FLAGS: u16 = 7;
// const TCA_ACT_HW_STATS: u16 = 8;
// const TCA_ACT_USED_HW_STATS: u16 = 9;
const TCA_ACT_IN_HW_COUNT: u16 = 10;

/// Attributes of a traffic control action.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionAttribute {
    /// The [`Kind`] (general type or class) of the action (e.g. "mirred",
    /// "nat").
    ///
    /// [`Kind`]: #variant.Kind
    Kind(String),
    /// Parameters of the action.
    Options(Vec<TcActionOption>),
    /// Index of the action.
    ///
    /// This is used to identify the action in the kernel.
    /// Each action [`Kind`] has a unique table of actions.
    /// That is, each action [`Kind`] has its own set of [`Index`] values.
    ///
    /// If [`Index`] is zero on action creation,
    /// the kernel will assign a unique index to the new action.
    /// The combination of [`Kind`] and [`Index`] can then be used to identify
    /// and interact with the action in the future.
    ///
    /// For example, one action can be used by multiple different filters by
    /// referencing the action's [`Index`] when creating that filter.
    /// Such multiply referenced actions will aggregate their statistics.
    ///
    /// The kernel will reject attempts to delete an action if it is in use by
    /// a filter.
    /// Remove all referencing filters before deleting the action.
    ///
    /// [`Kind`]: #variant.Kind
    /// [`Index`]: #variant.Index
    Index(u32),
    /// Statistics about the action (e.g., number of bytes and or packets
    /// processed).
    Stats(Vec<TcStats2>),
    /// [`Cookie`] is an attribute which _is not interpreted by the kernel at
    /// all_ and may be used to store up to 16 bytes of arbitrary data on
    /// an action in the kernel.
    /// Userspace processes may then use this data to store additional
    /// information about the action or to correlate actions with other
    /// data.
    ///
    /// [`Cookie`]: #variant.Cookie
    Cookie(Vec<u8>),
    /// Number of times the action has been installed in hardware.
    InHwCount(u32),
    /// Other attributes unknown at the time of writing or not yet supported by
    /// this library.
    Other(DefaultNla),
}

impl Nla for TcActionAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Cookie(bytes) => bytes.len(),
            Self::Kind(k) => k.len() + 1,
            Self::Options(opt) => opt.as_slice().buffer_len(),
            Self::Index(_) | Self::InHwCount(_) => 4,
            Self::Stats(s) => s.as_slice().buffer_len(),
            Self::Other(attr) => attr.value_len(),
        }
    }
    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Cookie(bytes) => buffer.copy_from_slice(bytes.as_slice()),
            Self::Kind(string) => {
                buffer[..string.len()].copy_from_slice(string.as_bytes());
                buffer[string.len()] = 0;
            }
            Self::Options(opt) => opt.as_slice().emit(buffer),
            Self::Index(value) | Self::InHwCount(value) => {
                NativeEndian::write_u32(buffer, *value);
            }
            Self::Stats(s) => s.as_slice().emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }
    fn kind(&self) -> u16 {
        match self {
            Self::Kind(_) => TCA_ACT_KIND,
            Self::Options(_) => TCA_ACT_OPTIONS | NLA_F_NESTED,
            Self::Index(_) => TCA_ACT_INDEX,
            Self::Stats(_) => TCA_ACT_STATS,
            Self::Cookie(_) => TCA_ACT_COOKIE,
            Self::InHwCount(_) => TCA_ACT_IN_HW_COUNT,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T, P> ParseableParametrized<NlaBuffer<&'a T>, P> for TcActionAttribute
where
    T: AsRef<[u8]> + ?Sized,
    P: AsRef<str>,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: P,
    ) -> Result<Self, DecodeError> {
        Ok(match buf.kind() {
            TCA_ACT_KIND => {
                let buf_value = buf.value();
                TcActionAttribute::Kind(
                    parse_string(buf_value)
                        .context("failed to parse TCA_ACT_KIND")?,
                )
            }
            TCA_ACT_OPTIONS => TcActionAttribute::Options(
                NlasIterator::new(buf.value())
                    .map(|nla| {
                        let nla = nla.context("invalid TCA_ACT_OPTIONS")?;
                        TcActionOption::parse_with_param(&nla, kind.as_ref())
                            .context("failed to parse TCA_ACT_OPTIONS")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TCA_ACT_INDEX => TcActionAttribute::Index(
                parse_u32(buf.value())
                    .context("failed to parse TCA_ACT_INDEX")?,
            ),
            TCA_ACT_STATS => TcActionAttribute::Stats(
                NlasIterator::new(buf.value())
                    .map(|nla| {
                        let nla = nla.context("invalid TCA_ACT_STATS")?;
                        TcStats2::parse_with_param(&nla, kind.as_ref())
                            .context("failed to parse TCA_ACT_STATS")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            TCA_ACT_COOKIE => TcActionAttribute::Cookie(buf.value().to_vec()),
            TCA_ACT_IN_HW_COUNT => TcActionAttribute::InHwCount(
                parse_u32(buf.value())
                    .context("failed to parse TCA_ACT_IN_HW_COUNT")?,
            ),
            _ => TcActionAttribute::Other(
                DefaultNla::parse(buf).context("failed to parse action nla")?,
            ),
        })
    }
}

/// [`TcActionOption`] is a netlink message attribute that describes an option
/// of a [tc-actions] action.
///
/// This enum is non-exhaustive as new action types may be added to the kernel
/// at any time.
/// Only a small subset of possible actions are currently supported.
///
/// [tc-actions]: https://man7.org/linux/man-pages/man8/tc-actions.8.html
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionOption {
    /// Mirror options.
    ///
    /// These options can be used to mirror (copy) or redirect frames / packets
    /// to another network interface.
    Mirror(TcActionMirrorOption),
    /// NAT options.
    ///
    /// These options type can be used to perform network address translation.
    Nat(TcActionNatOption),
    /// Tunnel key options.
    ///
    /// These options type can be used to assign encapsulation properties to
    /// the packet.
    TunnelKey(TcActionTunnelKeyOption),
    /// Other action types not yet supported by this library.
    Other(DefaultNla),
}

impl Nla for TcActionOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Mirror(nla) => nla.value_len(),
            Self::Nat(nla) => nla.value_len(),
            Self::TunnelKey(nla) => nla.value_len(),
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Mirror(nla) => nla.emit_value(buffer),
            Self::Nat(nla) => nla.emit_value(buffer),
            Self::TunnelKey(nla) => nla.emit_value(buffer),
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Mirror(nla) => nla.kind(),
            Self::Nat(nla) => nla.kind(),
            Self::TunnelKey(nla) => nla.kind(),
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T, S> ParseableParametrized<NlaBuffer<&'a T>, S> for TcActionOption
where
    T: AsRef<[u8]> + ?Sized,
    S: AsRef<str>,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: S,
    ) -> Result<Self, DecodeError> {
        Ok(match kind.as_ref() {
            TcActionMirror::KIND => Self::Mirror(
                TcActionMirrorOption::parse(buf)
                    .context("failed to parse mirror action")?,
            ),
            TcActionNat::KIND => Self::Nat(
                TcActionNatOption::parse(buf)
                    .context("failed to parse nat action")?,
            ),
            TcActionTunnelKey::KIND => Self::TunnelKey(
                TcActionTunnelKeyOption::parse(buf)
                    .context("failed to parse tunnel_key action")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(buf)
                    .context("failed to parse action options")?,
            ),
        })
    }
}

/// Generic traffic control action parameters.
///
/// This structure is used to describe attributes common to all traffic control
/// actions.
///
/// See [`#define tc_gen` in `linux/pkt_cls.h`][`tc_gen`].
///
/// [`tc_gen`]: https://elixir.bootlin.com/linux/v6.8.9/source/include/uapi/linux/pkt_cls.h#L179
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub struct TcActionGeneric {
    /// The [`index`] of the action is a unique identifier used to track
    /// actions installed in the kernel.
    ///
    /// Each action type (e.g. [`mirror`] or [`nat`]) has its own independent
    /// [`index`] space.
    /// If you assign the [`index`] field to `0` when creating an action, the
    /// kernel will assign a unique [`index`] to the new action.
    ///
    /// [`mirror`]: struct.TcActionMirror.html
    /// [`nat`]: struct.TcActionNat.html
    /// [`index`]: #structfield.index
    pub index: u32,
    /// NOTE: I cannot find any documentation on this field nor any place
    /// where it is used in iproute2 or the Linux kernel.
    /// The [`capab`] field is part of the [`#define tc_gen`] in the kernel,
    /// and that `#define` is used in many places,
    /// but I don't see any place using the [`capab`] field in any way.
    /// I may be looking in the wrong place or missing something.
    ///
    /// [`#define tc_gen`]: https://elixir.bootlin.com/linux/v6.8.9/source/include/uapi/linux/pkt_cls.h#L179
    /// [`capab`]: #structfield.capab
    pub capab: u32,
    /// Action type.
    pub action: TcActionType,
    /// Reference count of this action.
    ///
    /// This refers to the number of times this action is referenced within the
    /// kernel.
    /// Actions are cleaned up (deleted) when [`refcnt`] reaches 0.
    ///
    /// If you create an action on its own (i.e., not associated with a
    /// filter), the [`refcnt`] will be 1.
    /// If that action is then associated with a filter, the [`refcnt`] will be
    /// 2.
    /// If you then delete that filter, the [`refcnt`] will be 1 and the action
    /// will remain until you explicitly delete it (which is only possible
    /// when the [`refcnt`] is 1 and the [`bindcnt`] is 0).
    ///
    /// If you were to create an action indirectly (e.g., as part of creating a
    /// filter) then the [`refcnt`] will still be 1 (along with the
    /// [`bindcnt`]).
    /// If you then create another filter that references the same action, the
    /// [`refcnt`] will be 2 (along with the [`bindcnt`]).
    ///
    /// If you then deleted both of those actions,
    /// the [`refcnt`] would be 0 and the action would be removed from the
    /// kernel.
    ///
    /// [`refcnt`]: #structfield.refcnt
    /// [`bindcnt`]: #structfield.bindcnt
    pub refcnt: i32,
    /// Bind count of this action.
    ///
    /// The number of filters that reference (bind to) this action.
    pub bindcnt: i32,
}

impl TcActionGeneric {
    pub(crate) const BUF_LEN: usize = 20;
}

buffer!(TcActionGenericBuffer(TcActionGeneric::BUF_LEN) {
    index: (u32, 0..4),
    capab: (u32, 4..8),
    action: (i32, 8..12),
    refcnt: (i32, 12..16),
    bindcnt: (i32, 16..20),
});

impl Emitable for TcActionGeneric {
    fn buffer_len(&self) -> usize {
        Self::BUF_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcActionGenericBuffer::new(buffer);
        packet.set_index(self.index);
        packet.set_capab(self.capab);
        packet.set_action(self.action.into());
        packet.set_refcnt(self.refcnt);
        packet.set_bindcnt(self.bindcnt);
    }
}

impl<T: AsRef<[u8]>> Parseable<TcActionGenericBuffer<T>> for TcActionGeneric {
    fn parse(buf: &TcActionGenericBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            index: buf.index(),
            capab: buf.capab(),
            action: buf.action().into(),
            refcnt: buf.refcnt(),
            bindcnt: buf.bindcnt(),
        })
    }
}

const TC_ACT_UNSPEC: i32 = -1;
const TC_ACT_OK: i32 = 0;
const TC_ACT_RECLASSIFY: i32 = 1;
const TC_ACT_SHOT: i32 = 2;
const TC_ACT_PIPE: i32 = 3;
const TC_ACT_STOLEN: i32 = 4;
const TC_ACT_QUEUED: i32 = 5;
const TC_ACT_REPEAT: i32 = 6;
const TC_ACT_REDIRECT: i32 = 7;
const TC_ACT_TRAP: i32 = 8;

/// Generic traffic control action types.
///
/// These are the possible "outcomes" for a packet after an action is applied to
/// it.
///
/// This enum is non-exhaustive as new action types may be added to the kernel
/// at any time.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum TcActionType {
    /// No specific outcome specified (i.e., take the default for that action).
    #[default]
    Unspec,
    /// Terminates packet processing and allows the packet to proceed.
    Ok,
    /// Terminates packet processing and restart packet classification.
    Reclassify,
    /// Drop the packet.
    Shot,
    /// Pipe the packet to the next action (if any).
    Pipe,
    /// The packet is removed from this processing pipeline and returned to
    /// another.
    /// This happens, for example, when using the "mirred" redirect action.
    Stolen,
    /// Queue the packet for later processing.
    Queued,
    /// Repeat the action.
    ///
    /// > TODO: confirm this. I have not used this action before and its
    /// > semantics are unclear.
    Repeat,
    /// Redirect the packet.
    ///
    /// > TODO: confirm semantics of this action. It is unclear how
    /// > [`Redirect`] differs from [`Stolen`].
    ///
    /// [`Stolen`]: #variant.Stolen
    /// [`Redirect`]: #variant.Redirect
    Redirect,
    /// Transition packet processing from the hardware to software.
    ///
    /// If this action is encountered by in software, it is equivalent to
    /// [`Shot`].
    ///
    /// [`Shot`]: #variant.Shot
    Trap,
    /// Other action types not known at the time of writing or not yet
    /// supported by this library.
    Other(i32),
}

impl From<i32> for TcActionType {
    fn from(d: i32) -> Self {
        match d {
            TC_ACT_UNSPEC => Self::Unspec,
            TC_ACT_OK => Self::Ok,
            TC_ACT_RECLASSIFY => Self::Reclassify,
            TC_ACT_SHOT => Self::Shot,
            TC_ACT_PIPE => Self::Pipe,
            TC_ACT_STOLEN => Self::Stolen,
            TC_ACT_QUEUED => Self::Queued,
            TC_ACT_REPEAT => Self::Repeat,
            TC_ACT_REDIRECT => Self::Redirect,
            TC_ACT_TRAP => Self::Trap,
            _ => Self::Other(d),
        }
    }
}

impl From<TcActionType> for i32 {
    fn from(v: TcActionType) -> i32 {
        match v {
            TcActionType::Unspec => TC_ACT_UNSPEC,
            TcActionType::Ok => TC_ACT_OK,
            TcActionType::Reclassify => TC_ACT_RECLASSIFY,
            TcActionType::Shot => TC_ACT_SHOT,
            TcActionType::Pipe => TC_ACT_PIPE,
            TcActionType::Stolen => TC_ACT_STOLEN,
            TcActionType::Queued => TC_ACT_QUEUED,
            TcActionType::Repeat => TC_ACT_REPEAT,
            TcActionType::Redirect => TC_ACT_REDIRECT,
            TcActionType::Trap => TC_ACT_TRAP,
            TcActionType::Other(d) => d,
        }
    }
}

pub const TC_TCF_BUF_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Tcf {
    pub install: u64,
    pub lastuse: u64,
    pub expires: u64,
    pub firstuse: u64,
}

// kernel struct `tcf_t`
buffer!(TcfBuffer(TC_TCF_BUF_LEN) {
    install: (u64, 0..8),
    lastuse: (u64, 8..16),
    expires: (u64, 16..24),
    firstuse: (u64, 24..32),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<TcfBuffer<&T>> for Tcf {
    fn parse(buf: &TcfBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            install: buf.install(),
            lastuse: buf.lastuse(),
            expires: buf.expires(),
            firstuse: buf.firstuse(),
        })
    }
}

impl Emitable for Tcf {
    fn buffer_len(&self) -> usize {
        TC_TCF_BUF_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcfBuffer::new(buffer);
        packet.set_install(self.install);
        packet.set_lastuse(self.lastuse);
        packet.set_expires(self.expires);
        packet.set_firstuse(self.firstuse);
    }
}
