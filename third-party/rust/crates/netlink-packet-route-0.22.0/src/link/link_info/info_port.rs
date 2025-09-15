// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{Nla, NlaBuffer, NlasIterator},
    parsers::parse_string,
    DecodeError, Emitable, Parseable,
};

use super::{
    super::{InfoBondPort, InfoBridgePort},
    InfoVrf,
};

const BOND: &str = "bond";
const BRIDGE: &str = "bridge";
const VRF: &str = "vrf";

const IFLA_INFO_PORT_KIND: u16 = 4;
const IFLA_INFO_PORT_DATA: u16 = 5;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoPortKind {
    Bond,
    Bridge,
    Vrf,
    Other(String),
}

impl std::fmt::Display for InfoPortKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Bond => BOND,
                Self::Bridge => BRIDGE,
                Self::Vrf => VRF,
                Self::Other(s) => s.as_str(),
            }
        )
    }
}

impl Nla for InfoPortKind {
    fn value_len(&self) -> usize {
        let len = match self {
            Self::Bond => BOND.len(),
            Self::Bridge => BRIDGE.len(),
            Self::Vrf => VRF.len(),
            Self::Other(s) => s.len(),
        };
        len + 1
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        let s = match self {
            Self::Bond => BOND,
            Self::Bridge => BRIDGE,
            Self::Vrf => VRF,
            Self::Other(s) => s.as_str(),
        };
        buffer[..s.len()].copy_from_slice(s.as_bytes());
        buffer[s.len()] = 0;
    }

    fn kind(&self) -> u16 {
        IFLA_INFO_PORT_KIND
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoPortKind {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<InfoPortKind, DecodeError> {
        if buf.kind() != IFLA_INFO_PORT_KIND {
            return Err(format!(
                "failed to parse IFLA_INFO_PORT_KIND: NLA type is {}",
                buf.kind()
            )
            .into());
        }
        let s = parse_string(buf.value())
            .context("invalid IFLA_INFO_PORT_KIND value")?;
        Ok(match s.as_str() {
            BOND => Self::Bond,
            BRIDGE => Self::Bridge,
            VRF => Self::Vrf,
            _ => Self::Other(s),
        })
    }
}

pub type InfoVrfPort = InfoVrf;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoPortData {
    BondPort(Vec<InfoBondPort>),
    BridgePort(Vec<InfoBridgePort>),
    VrfPort(Vec<InfoVrfPort>),
    Other(Vec<u8>),
}

impl Nla for InfoPortData {
    fn value_len(&self) -> usize {
        match self {
            Self::BondPort(nlas) => nlas.as_slice().buffer_len(),
            Self::BridgePort(nlas) => nlas.as_slice().buffer_len(),
            Self::VrfPort(nlas) => nlas.as_slice().buffer_len(),
            Self::Other(bytes) => bytes.len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::BondPort(nlas) => nlas.as_slice().emit(buffer),
            Self::BridgePort(nlas) => nlas.as_slice().emit(buffer),
            Self::VrfPort(nlas) => nlas.as_slice().emit(buffer),
            Self::Other(bytes) => buffer.copy_from_slice(bytes),
        }
    }

    fn kind(&self) -> u16 {
        IFLA_INFO_PORT_DATA
    }
}

impl InfoPortData {
    pub(crate) fn parse_with_param(
        payload: &[u8],
        kind: InfoPortKind,
    ) -> Result<InfoPortData, DecodeError> {
        let port_data = match kind {
            InfoPortKind::Bond => NlasIterator::new(payload)
                .map(|nla| nla.and_then(|nla| InfoBondPort::parse(&nla)))
                .collect::<Result<Vec<_>, _>>()
                .map(InfoPortData::BondPort),
            InfoPortKind::Bridge => NlasIterator::new(payload)
                .map(|nla| nla.and_then(|nla| InfoBridgePort::parse(&nla)))
                .collect::<Result<Vec<_>, _>>()
                .map(InfoPortData::BridgePort),
            InfoPortKind::Vrf => NlasIterator::new(payload)
                .map(|nla| nla.and_then(|nla| InfoVrfPort::parse(&nla)))
                .collect::<Result<Vec<_>, _>>()
                .map(InfoPortData::VrfPort),
            InfoPortKind::Other(_) => Ok(InfoPortData::Other(payload.to_vec())),
        };

        Ok(port_data.context(format!(
            "failed to parse IFLA_INFO_PORT_DATA (IFLA_INFO_PORT_KIND is '{kind}')"
        ))?)
    }
}
