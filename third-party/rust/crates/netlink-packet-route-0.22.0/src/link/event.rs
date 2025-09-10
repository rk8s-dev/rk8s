// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    parsers::parse_u32, DecodeError, Emitable, Parseable,
};

const IFLA_EVENT_NONE: u32 = 0;
const IFLA_EVENT_REBOOT: u32 = 1;
const IFLA_EVENT_FEATURES: u32 = 2;
const IFLA_EVENT_BONDING_FAILOVER: u32 = 3;
const IFLA_EVENT_NOTIFY_PEERS: u32 = 4;
const IFLA_EVENT_IGMP_RESEND: u32 = 5;
const IFLA_EVENT_BONDING_OPTIONS: u32 = 6;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum LinkEvent {
    #[default]
    None,
    Reboot,
    Features,
    BondingFailover,
    NotifyPeers,
    IgmpResend,
    BondingOptions,
    Other(u32),
}

impl From<u32> for LinkEvent {
    fn from(d: u32) -> Self {
        match d {
            IFLA_EVENT_NONE => Self::None,
            IFLA_EVENT_REBOOT => Self::Reboot,
            IFLA_EVENT_FEATURES => Self::Features,
            IFLA_EVENT_BONDING_FAILOVER => Self::BondingFailover,
            IFLA_EVENT_NOTIFY_PEERS => Self::NotifyPeers,
            IFLA_EVENT_IGMP_RESEND => Self::IgmpResend,
            IFLA_EVENT_BONDING_OPTIONS => Self::BondingOptions,
            _ => Self::Other(d),
        }
    }
}

impl From<LinkEvent> for u32 {
    fn from(v: LinkEvent) -> u32 {
        match v {
            LinkEvent::None => IFLA_EVENT_NONE,
            LinkEvent::Reboot => IFLA_EVENT_REBOOT,
            LinkEvent::Features => IFLA_EVENT_FEATURES,
            LinkEvent::BondingFailover => IFLA_EVENT_BONDING_FAILOVER,
            LinkEvent::NotifyPeers => IFLA_EVENT_NOTIFY_PEERS,
            LinkEvent::IgmpResend => IFLA_EVENT_IGMP_RESEND,
            LinkEvent::BondingOptions => IFLA_EVENT_BONDING_OPTIONS,
            LinkEvent::Other(d) => d,
        }
    }
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<T> for LinkEvent {
    fn parse(buf: &T) -> Result<Self, DecodeError> {
        Ok(LinkEvent::from(
            parse_u32(buf.as_ref()).context("invalid IFLA_EVENT value")?,
        ))
    }
}

impl Emitable for LinkEvent {
    fn buffer_len(&self) -> usize {
        4
    }

    fn emit(&self, buffer: &mut [u8]) {
        NativeEndian::write_u32(buffer, u32::from(*self));
    }
}
