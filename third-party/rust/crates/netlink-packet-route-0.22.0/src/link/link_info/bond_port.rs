// SPDX-License-Identifier: MIT
use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_i32, parse_u16, parse_u32, parse_u8},
    traits::Parseable,
    DecodeError,
};

const IFLA_BOND_PORT_STATE_ACTIVE: u8 = 0;
const IFLA_BOND_PORT_STATE_BACKUP: u8 = 1;

const IFLA_BOND_PORT_MII_STATUS_UP: u8 = 0;
const IFLA_BOND_PORT_MII_STATUS_GOING_DOWN: u8 = 1;
const IFLA_BOND_PORT_MII_STATUS_DOWN: u8 = 2;
const IFLA_BOND_PORT_MII_STATUS_GOING_BACK: u8 = 3;

const IFLA_BOND_PORT_STATE: u16 = 1;
const IFLA_BOND_PORT_MII_STATUS: u16 = 2;
const IFLA_BOND_PORT_LINK_FAILURE_COUNT: u16 = 3;
const IFLA_BOND_PORT_PERM_HWADDR: u16 = 4;
const IFLA_BOND_PORT_QUEUE_ID: u16 = 5;
// const IFLA_BOND_PORT_AD_AGGREGATOR_ID: u16 = 6;
// const IFLA_BOND_PORT_AD_ACTOR_OPER_PORT_STATE: u16 = 7;
// const IFLA_BOND_PORT_AD_PARTNER_OPER_PORT_STATE: u16 = 8;
const IFLA_BOND_PORT_PRIO: u16 = 9;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum BondPortState {
    Active,
    Backup,
    Other(u8),
}

impl From<u8> for BondPortState {
    fn from(value: u8) -> Self {
        use self::BondPortState::*;
        match value {
            IFLA_BOND_PORT_STATE_ACTIVE => Active,
            IFLA_BOND_PORT_STATE_BACKUP => Backup,
            _ => Other(value),
        }
    }
}

impl From<BondPortState> for u8 {
    fn from(value: BondPortState) -> Self {
        use self::BondPortState::*;
        match value {
            Active => IFLA_BOND_PORT_STATE_ACTIVE,
            Backup => IFLA_BOND_PORT_STATE_BACKUP,
            Other(other) => other,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum MiiStatus {
    Up,
    GoingDown,
    Down,
    GoingBack,
    Other(u8),
}

impl From<u8> for MiiStatus {
    fn from(value: u8) -> Self {
        use self::MiiStatus::*;
        match value {
            IFLA_BOND_PORT_MII_STATUS_UP => Up,
            IFLA_BOND_PORT_MII_STATUS_GOING_DOWN => GoingDown,
            IFLA_BOND_PORT_MII_STATUS_DOWN => Down,
            IFLA_BOND_PORT_MII_STATUS_GOING_BACK => GoingBack,
            _ => Other(value),
        }
    }
}

impl From<MiiStatus> for u8 {
    fn from(value: MiiStatus) -> Self {
        use self::MiiStatus::*;
        match value {
            Up => IFLA_BOND_PORT_MII_STATUS_UP,
            GoingDown => IFLA_BOND_PORT_MII_STATUS_GOING_DOWN,
            Down => IFLA_BOND_PORT_MII_STATUS_DOWN,
            GoingBack => IFLA_BOND_PORT_MII_STATUS_GOING_BACK,
            Other(other) => other,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoBondPort {
    LinkFailureCount(u32),
    MiiStatus(MiiStatus),
    PermHwaddr(Vec<u8>),
    Prio(i32),
    QueueId(u16),
    BondPortState(BondPortState),
    Other(DefaultNla),
}

impl Nla for InfoBondPort {
    #[rustfmt::skip]
    fn value_len(&self) -> usize {
        use self::InfoBondPort::*;
        match self {
            QueueId(_)
                => 2,
            LinkFailureCount(_) |
            Prio(_)
                => 4,
            PermHwaddr(ref bytes)
            => bytes.len(),
            MiiStatus(_) => 1,
            BondPortState(_) => 1,
            Other(nla)
                => nla.value_len(),
        }
    }

    #[rustfmt::skip]
    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoBondPort::*;
        match self {
            QueueId(ref value)
             => NativeEndian::write_u16(buffer, *value),
            PermHwaddr(ref bytes)
             => buffer.copy_from_slice(bytes.as_slice()),
            Prio(ref value)
             => NativeEndian::write_i32(buffer, *value),
            LinkFailureCount(value)
             => NativeEndian::write_u32(buffer, *value),
            MiiStatus(state) => buffer[0] = (*state).into(),
            BondPortState(state) => buffer[0] = (*state).into(),
            Other(nla)
             => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoBondPort::*;

        match self {
            LinkFailureCount(_) => IFLA_BOND_PORT_LINK_FAILURE_COUNT,
            MiiStatus(_) => IFLA_BOND_PORT_MII_STATUS,
            PermHwaddr(_) => IFLA_BOND_PORT_PERM_HWADDR,
            Prio(_) => IFLA_BOND_PORT_PRIO,
            QueueId(_) => IFLA_BOND_PORT_QUEUE_ID,
            BondPortState(_) => IFLA_BOND_PORT_STATE,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoBondPort {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoBondPort::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BOND_PORT_LINK_FAILURE_COUNT => {
                LinkFailureCount(parse_u32(payload).context(
                    "invalid IFLA_BOND_PORT_LINK_FAILURE_COUNT value",
                )?)
            }
            IFLA_BOND_PORT_MII_STATUS => MiiStatus(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_PORT_MII_STATUS value")?
                    .into(),
            ),
            IFLA_BOND_PORT_PERM_HWADDR => PermHwaddr(payload.to_vec()),
            IFLA_BOND_PORT_PRIO => Prio(
                parse_i32(payload)
                    .context("invalid IFLA_BOND_PORT_PRIO value")?,
            ),
            IFLA_BOND_PORT_QUEUE_ID => QueueId(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_PORT_QUEUE_ID value")?,
            ),
            IFLA_BOND_PORT_STATE => BondPortState(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_PORT_STATE value")?
                    .into(),
            ),
            kind => Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
