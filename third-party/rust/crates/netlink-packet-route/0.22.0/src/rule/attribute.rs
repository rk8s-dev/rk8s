// SPDX-License-Identifier: MIT

use std::net::IpAddr;

use anyhow::Context;
use netlink_packet_utils::{
    byteorder::{ByteOrder, NativeEndian},
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_string, parse_u32, parse_u8},
    DecodeError, Emitable, Parseable,
};

use crate::{
    ip::{emit_ip_addr, ip_addr_len, parse_ip_addr, IpProtocol},
    route::{RouteProtocol, RouteRealm},
    rule::{RulePortRange, RuleUidRange},
};

const FRA_DST: u16 = 1;
const FRA_SRC: u16 = 2;
const FRA_IIFNAME: u16 = 3;
const FRA_GOTO: u16 = 4;
// const FRA_UNUSED2: u16 = 5;
const FRA_PRIORITY: u16 = 6;
// const FRA_UNUSED3: u16 = 7;
// const FRA_UNUSED4: u16 = 8;
// const FRA_UNUSED5: u16 = 9;
const FRA_FWMARK: u16 = 10;
const FRA_FLOW: u16 = 11;
const FRA_TUN_ID: u16 = 12;
const FRA_SUPPRESS_IFGROUP: u16 = 13;
const FRA_SUPPRESS_PREFIXLEN: u16 = 14;
const FRA_TABLE: u16 = 15;
const FRA_FWMASK: u16 = 16;
const FRA_OIFNAME: u16 = 17;
// const FRA_PAD: u16 = 18;
const FRA_L3MDEV: u16 = 19;
const FRA_UID_RANGE: u16 = 20;
const FRA_PROTOCOL: u16 = 21;
const FRA_IP_PROTO: u16 = 22;
const FRA_SPORT_RANGE: u16 = 23;
const FRA_DPORT_RANGE: u16 = 24;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RuleAttribute {
    /// destination address
    Destination(IpAddr),
    /// source address
    Source(IpAddr),
    /// input interface name
    Iifname(String),
    /// The priority number of another rule for [super::RuleAction::Goto]
    Goto(u32),
    Priority(u32),
    FwMark(u32),
    FwMask(u32),
    /// IPv4 route realm
    Realm(RouteRealm),
    TunId(u32),
    SuppressIfGroup(u32),
    SuppressPrefixLen(u32),
    Table(u32),
    /// output interface name
    Oifname(String),
    L3MDev(bool),
    UidRange(RuleUidRange),
    Protocol(RouteProtocol),
    IpProtocol(IpProtocol),
    SourcePortRange(RulePortRange),
    DestinationPortRange(RulePortRange),
    Other(DefaultNla),
}

impl Nla for RuleAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Destination(ip) | Self::Source(ip) => ip_addr_len(ip),
            Self::UidRange(v) => v.buffer_len(),
            Self::SourcePortRange(v) | Self::DestinationPortRange(v) => {
                v.buffer_len()
            }
            Self::Iifname(s) | Self::Oifname(s) => s.len() + 1,
            Self::Priority(_)
            | Self::FwMark(_)
            | Self::FwMask(_)
            | Self::TunId(_)
            | Self::Goto(_)
            | Self::SuppressIfGroup(_)
            | Self::SuppressPrefixLen(_)
            | Self::Table(_) => 4,
            Self::Realm(v) => v.buffer_len(),
            Self::L3MDev(_) | Self::Protocol(_) | Self::IpProtocol(_) => 1,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Destination(_) => FRA_DST,
            Self::Source(_) => FRA_SRC,
            Self::Iifname(_) => FRA_IIFNAME,
            Self::Goto(_) => FRA_GOTO,
            Self::Priority(_) => FRA_PRIORITY,
            Self::FwMark(_) => FRA_FWMARK,
            Self::FwMask(_) => FRA_FWMASK,
            Self::Realm(_) => FRA_FLOW,
            Self::TunId(_) => FRA_TUN_ID,
            Self::SuppressIfGroup(_) => FRA_SUPPRESS_IFGROUP,
            Self::SuppressPrefixLen(_) => FRA_SUPPRESS_PREFIXLEN,
            Self::Table(_) => FRA_TABLE,
            Self::Oifname(_) => FRA_OIFNAME,
            Self::L3MDev(_) => FRA_L3MDEV,
            Self::UidRange(_) => FRA_UID_RANGE,
            Self::Protocol(_) => FRA_PROTOCOL,
            Self::IpProtocol(_) => FRA_IP_PROTO,
            Self::SourcePortRange(_) => FRA_SPORT_RANGE,
            Self::DestinationPortRange(_) => FRA_DPORT_RANGE,
            Self::Other(attr) => attr.kind(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Destination(ip) | Self::Source(ip) => {
                emit_ip_addr(ip, buffer)
            }
            Self::SourcePortRange(v) | Self::DestinationPortRange(v) => {
                v.emit(buffer)
            }
            Self::UidRange(v) => v.emit(buffer),
            Self::Iifname(s) | Self::Oifname(s) => {
                buffer[..s.len()].copy_from_slice(s.as_bytes())
            }
            Self::Realm(v) => v.emit(buffer),
            Self::Priority(value)
            | Self::FwMark(value)
            | Self::FwMask(value)
            | Self::TunId(value)
            | Self::Goto(value)
            | Self::SuppressIfGroup(value)
            | Self::SuppressPrefixLen(value)
            | Self::Table(value) => NativeEndian::write_u32(buffer, *value),
            Self::L3MDev(value) => buffer[0] = (*value).into(),
            Self::IpProtocol(value) => buffer[0] = i32::from(*value) as u8,
            Self::Protocol(value) => buffer[0] = u8::from(*value),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for RuleAttribute
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();

        Ok(match buf.kind() {
            FRA_DST => Self::Destination(
                parse_ip_addr(payload)
                    .context(format!("Invalid FRA_DST value {payload:?}"))?,
            ),
            FRA_SRC => Self::Source(
                parse_ip_addr(payload)
                    .context(format!("Invalid FRA_SRC value {payload:?}"))?,
            ),
            FRA_IIFNAME => Self::Iifname(
                parse_string(payload).context("invalid FRA_IIFNAME value")?,
            ),
            FRA_GOTO => Self::Goto(
                parse_u32(payload).context("invalid FRA_GOTO value")?,
            ),
            FRA_PRIORITY => Self::Priority(
                parse_u32(payload).context("invalid FRA_PRIORITY value")?,
            ),
            FRA_FWMARK => Self::FwMark(
                parse_u32(payload).context("invalid FRA_FWMARK value")?,
            ),
            FRA_FLOW => Self::Realm(
                RouteRealm::parse(payload).context("invalid FRA_FLOW value")?,
            ),
            FRA_TUN_ID => Self::TunId(
                parse_u32(payload).context("invalid FRA_TUN_ID value")?,
            ),
            FRA_SUPPRESS_IFGROUP => Self::SuppressIfGroup(
                parse_u32(payload)
                    .context("invalid FRA_SUPPRESS_IFGROUP value")?,
            ),
            FRA_SUPPRESS_PREFIXLEN => Self::SuppressPrefixLen(
                parse_u32(payload)
                    .context("invalid FRA_SUPPRESS_PREFIXLEN value")?,
            ),
            FRA_TABLE => Self::Table(
                parse_u32(payload).context("invalid FRA_TABLE value")?,
            ),
            FRA_FWMASK => Self::FwMask(
                parse_u32(payload).context("invalid FRA_FWMASK value")?,
            ),
            FRA_OIFNAME => Self::Oifname(
                parse_string(payload).context("invalid FRA_OIFNAME value")?,
            ),
            FRA_L3MDEV => Self::L3MDev(
                parse_u8(payload).context("invalid FRA_L3MDEV value")? > 0,
            ),
            FRA_UID_RANGE => Self::UidRange(
                RuleUidRange::parse(payload)
                    .context("invalid FRA_UID_RANGE value")?,
            ),
            FRA_PROTOCOL => Self::Protocol(
                parse_u8(payload)
                    .context("invalid FRA_PROTOCOL value")?
                    .into(),
            ),
            FRA_IP_PROTO => Self::IpProtocol(IpProtocol::from(
                parse_u8(payload).context("invalid FRA_IP_PROTO value")? as i32,
            )),
            FRA_SPORT_RANGE => Self::SourcePortRange(
                RulePortRange::parse(payload)
                    .context("invalid FRA_SPORT_RANGE value")?,
            ),
            FRA_DPORT_RANGE => Self::DestinationPortRange(
                RulePortRange::parse(payload)
                    .context("invalid FRA_DPORT_RANGE value")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(buf).context("invalid NLA (unknown kind)")?,
            ),
        })
    }
}
