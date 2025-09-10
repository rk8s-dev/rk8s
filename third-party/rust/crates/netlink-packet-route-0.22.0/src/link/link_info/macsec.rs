// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16, parse_u32, parse_u64, parse_u8},
    traits::Parseable,
    DecodeError,
};

const IFLA_MACSEC_SCI: u16 = 1;
const IFLA_MACSEC_PORT: u16 = 2;
const IFLA_MACSEC_ICV_LEN: u16 = 3;
const IFLA_MACSEC_CIPHER_SUITE: u16 = 4;
const IFLA_MACSEC_WINDOW: u16 = 5;
const IFLA_MACSEC_ENCODING_SA: u16 = 6;
const IFLA_MACSEC_ENCRYPT: u16 = 7;
const IFLA_MACSEC_PROTECT: u16 = 8;
const IFLA_MACSEC_INC_SCI: u16 = 9;
const IFLA_MACSEC_ES: u16 = 10;
const IFLA_MACSEC_SCB: u16 = 11;
const IFLA_MACSEC_REPLAY_PROTECT: u16 = 12;
const IFLA_MACSEC_VALIDATION: u16 = 13;
// const IFLA_MACSEC_PAD: u16 = 14;
const IFLA_MACSEC_OFFLOAD: u16 = 15;
const MACSEC_VALIDATE_DISABLED: u8 = 0;
const MACSEC_VALIDATE_CHECK: u8 = 1;
const MACSEC_VALIDATE_STRICT: u8 = 2;
const MACSEC_OFFLOAD_OFF: u8 = 0;
const MACSEC_OFFLOAD_PHY: u8 = 1;
const MACSEC_OFFLOAD_MAC: u8 = 2;
const MACSEC_CIPHER_ID_GCM_AES_128: u64 = 0x0080C20001000001;
const MACSEC_CIPHER_ID_GCM_AES_256: u64 = 0x0080C20001000002;
const MACSEC_CIPHER_ID_GCM_AES_XPN_128: u64 = 0x0080C20001000003;
const MACSEC_CIPHER_ID_GCM_AES_XPN_256: u64 = 0x0080C20001000004;
const MACSEC_DEFAULT_CIPHER_ID: u64 = 0x0080020001000001;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum MacSecCipherId {
    #[deprecated]
    DefaultGcmAes128,
    GcmAes128,
    GcmAes256,
    GcmAesXpn128,
    GcmAesXpn256,
    Other(u64),
}

impl From<u64> for MacSecCipherId {
    fn from(d: u64) -> Self {
        match d {
            #[allow(deprecated)]
            MACSEC_DEFAULT_CIPHER_ID => Self::DefaultGcmAes128,
            MACSEC_CIPHER_ID_GCM_AES_128 => Self::GcmAes128,
            MACSEC_CIPHER_ID_GCM_AES_256 => Self::GcmAes256,
            MACSEC_CIPHER_ID_GCM_AES_XPN_128 => Self::GcmAesXpn128,
            MACSEC_CIPHER_ID_GCM_AES_XPN_256 => Self::GcmAesXpn256,
            _ => Self::Other(d),
        }
    }
}

impl From<MacSecCipherId> for u64 {
    fn from(d: MacSecCipherId) -> Self {
        match d {
            #[allow(deprecated)]
            MacSecCipherId::DefaultGcmAes128 => MACSEC_DEFAULT_CIPHER_ID,
            MacSecCipherId::GcmAes128 => MACSEC_CIPHER_ID_GCM_AES_128,
            MacSecCipherId::GcmAes256 => MACSEC_CIPHER_ID_GCM_AES_256,
            MacSecCipherId::GcmAesXpn128 => MACSEC_CIPHER_ID_GCM_AES_XPN_128,
            MacSecCipherId::GcmAesXpn256 => MACSEC_CIPHER_ID_GCM_AES_XPN_256,
            MacSecCipherId::Other(value) => value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum MacSecValidate {
    Disabled,
    Check,
    Strict,
    Other(u8),
}

impl From<u8> for MacSecValidate {
    fn from(d: u8) -> Self {
        match d {
            MACSEC_VALIDATE_DISABLED => Self::Disabled,
            MACSEC_VALIDATE_CHECK => Self::Check,
            MACSEC_VALIDATE_STRICT => Self::Strict,
            _ => Self::Other(d),
        }
    }
}

impl From<MacSecValidate> for u8 {
    fn from(d: MacSecValidate) -> Self {
        match d {
            MacSecValidate::Disabled => MACSEC_VALIDATE_DISABLED,
            MacSecValidate::Check => MACSEC_VALIDATE_CHECK,
            MacSecValidate::Strict => MACSEC_VALIDATE_STRICT,
            MacSecValidate::Other(value) => value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum MacSecOffload {
    Off,
    Phy,
    Mac,
    Other(u8),
}

impl From<u8> for MacSecOffload {
    fn from(d: u8) -> Self {
        match d {
            MACSEC_OFFLOAD_OFF => Self::Off,
            MACSEC_OFFLOAD_PHY => Self::Phy,
            MACSEC_OFFLOAD_MAC => Self::Mac,
            _ => Self::Other(d),
        }
    }
}

impl From<MacSecOffload> for u8 {
    fn from(d: MacSecOffload) -> Self {
        match d {
            MacSecOffload::Off => MACSEC_OFFLOAD_OFF,
            MacSecOffload::Phy => MACSEC_OFFLOAD_PHY,
            MacSecOffload::Mac => MACSEC_OFFLOAD_MAC,
            MacSecOffload::Other(value) => value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoMacSec {
    Sci(u64),
    Port(u16),
    IcvLen(u8),
    CipherSuite(MacSecCipherId),
    Window(u32),
    EncodingSa(u8),
    Encrypt(u8),
    Protect(u8),
    IncSci(u8),
    Es(u8),
    Scb(u8),
    ReplayProtect(u8),
    Validation(MacSecValidate),
    Offload(MacSecOffload),
    Other(DefaultNla),
}

impl Nla for InfoMacSec {
    fn value_len(&self) -> usize {
        use self::InfoMacSec::*;
        match self {
            Sci(_) | CipherSuite(_) => 8,
            Window(_) => 4,
            Port(_) => 2,
            IcvLen(_) | EncodingSa(_) | Encrypt(_) | Protect(_) | IncSci(_)
            | Es(_) | Scb(_) | ReplayProtect(_) | Validation(_)
            | Offload(_) => 1,
            Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoMacSec::*;
        match self {
            Sci(value) => NativeEndian::write_u64(buffer, *value),
            CipherSuite(value) => {
                NativeEndian::write_u64(buffer, (*value).into())
            }
            Window(value) => NativeEndian::write_u32(buffer, *value),
            Port(value) => NativeEndian::write_u16(buffer, *value),
            IcvLen(value) | EncodingSa(value) | Encrypt(value)
            | Protect(value) | IncSci(value) | Es(value) | Scb(value)
            | ReplayProtect(value) => buffer[0] = *value,
            Offload(value) => buffer[0] = (*value).into(),
            Validation(value) => buffer[0] = (*value).into(),
            Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoMacSec::*;
        match self {
            Sci(_) => IFLA_MACSEC_SCI,
            Port(_) => IFLA_MACSEC_PORT,
            IcvLen(_) => IFLA_MACSEC_ICV_LEN,
            CipherSuite(_) => IFLA_MACSEC_CIPHER_SUITE,
            Window(_) => IFLA_MACSEC_WINDOW,
            EncodingSa(_) => IFLA_MACSEC_ENCODING_SA,
            Encrypt(_) => IFLA_MACSEC_ENCRYPT,
            Protect(_) => IFLA_MACSEC_PROTECT,
            IncSci(_) => IFLA_MACSEC_INC_SCI,
            Es(_) => IFLA_MACSEC_ES,
            Scb(_) => IFLA_MACSEC_SCB,
            ReplayProtect(_) => IFLA_MACSEC_REPLAY_PROTECT,
            Validation(_) => IFLA_MACSEC_VALIDATION,
            Offload(_) => IFLA_MACSEC_OFFLOAD,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoMacSec {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoMacSec::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_MACSEC_SCI => {
                Sci(parse_u64(payload)
                    .context("invalid IFLA_MACSEC_SCI value")?)
            }
            IFLA_MACSEC_PORT => Port(
                parse_u16(payload).context("invalid IFLA_MACSEC_PORT value")?,
            ),
            IFLA_MACSEC_ICV_LEN => IcvLen(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_ICV_LEN value")?,
            ),
            IFLA_MACSEC_CIPHER_SUITE => CipherSuite(
                parse_u64(payload)
                    .context("invalid IFLA_MACSEC_CIPHER_SUITE value")?
                    .into(),
            ),
            IFLA_MACSEC_WINDOW => Window(
                parse_u32(payload)
                    .context("invalid IFLA_MACSEC_WINDOW value")?,
            ),
            IFLA_MACSEC_ENCODING_SA => EncodingSa(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_ENCODING_SA value")?,
            ),
            IFLA_MACSEC_ENCRYPT => Encrypt(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_ENCRYPT value")?,
            ),
            IFLA_MACSEC_PROTECT => Protect(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_PROTECT value")?,
            ),
            IFLA_MACSEC_INC_SCI => IncSci(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_INC_SCI value")?,
            ),
            IFLA_MACSEC_ES => {
                Es(parse_u8(payload).context("invalid IFLA_MACSEC_ES value")?)
            }
            IFLA_MACSEC_SCB => {
                Scb(parse_u8(payload)
                    .context("invalid IFLA_MACSEC_SCB value")?)
            }
            IFLA_MACSEC_REPLAY_PROTECT => ReplayProtect(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_REPLAY_PROTECT value")?,
            ),
            IFLA_MACSEC_VALIDATION => Validation(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_VALIDATION value")?
                    .into(),
            ),
            IFLA_MACSEC_OFFLOAD => Offload(
                parse_u8(payload)
                    .context("invalid IFLA_MACSEC_OFFLOAD value")?
                    .into(),
            ),
            kind => Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
