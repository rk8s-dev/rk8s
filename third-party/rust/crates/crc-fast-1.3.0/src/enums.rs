// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

use crate::consts::*;
use crate::CrcAlgorithm;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

impl FromStr for CrcAlgorithm {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            NAME_CRC32_AIXM => Ok(CrcAlgorithm::Crc32Aixm),
            NAME_CRC32_AUTOSAR => Ok(CrcAlgorithm::Crc32Autosar),
            NAME_CRC32_BASE91_D => Ok(CrcAlgorithm::Crc32Base91D),
            NAME_CRC32_BZIP2 => Ok(CrcAlgorithm::Crc32Bzip2),
            NAME_CRC32_CD_ROM_EDC => Ok(CrcAlgorithm::Crc32CdRomEdc),
            NAME_CRC32_CKSUM => Ok(CrcAlgorithm::Crc32Cksum),
            NAME_CRC32_ISCSI => Ok(CrcAlgorithm::Crc32Iscsi),
            NAME_CRC32_ISO_HDLC => Ok(CrcAlgorithm::Crc32IsoHdlc),
            NAME_CRC32_JAMCRC => Ok(CrcAlgorithm::Crc32Jamcrc),
            NAME_CRC32_MEF => Ok(CrcAlgorithm::Crc32Mef),
            NAME_CRC32_MPEG_2 => Ok(CrcAlgorithm::Crc32Mpeg2),
            NAME_CRC32_XFER => Ok(CrcAlgorithm::Crc32Xfer),
            NAME_CRC64_GO_ISO => Ok(CrcAlgorithm::Crc64GoIso),
            NAME_CRC64_MS => Ok(CrcAlgorithm::Crc64Ms),
            NAME_CRC64_NVME => Ok(CrcAlgorithm::Crc64Nvme),
            NAME_CRC64_REDIS => Ok(CrcAlgorithm::Crc64Redis),
            NAME_CRC64_XZ => Ok(CrcAlgorithm::Crc64Xz),
            NAME_CRC64_ECMA_182 => Ok(CrcAlgorithm::Crc64Ecma182),
            NAME_CRC64_WE => Ok(CrcAlgorithm::Crc64We),
            _ => Err(()),
        }
    }
}

impl Display for CrcAlgorithm {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CrcAlgorithm::Crc32Aixm => write!(f, "{}", NAME_CRC32_AIXM),
            CrcAlgorithm::Crc32Autosar => write!(f, "{}", NAME_CRC32_AUTOSAR),
            CrcAlgorithm::Crc32Base91D => write!(f, "{}", NAME_CRC32_BASE91_D),
            CrcAlgorithm::Crc32Bzip2 => write!(f, "{}", NAME_CRC32_BZIP2),
            CrcAlgorithm::Crc32CdRomEdc => write!(f, "{}", NAME_CRC32_CD_ROM_EDC),
            CrcAlgorithm::Crc32Cksum => write!(f, "{}", NAME_CRC32_CKSUM),
            CrcAlgorithm::Crc32Iscsi => write!(f, "{}", NAME_CRC32_ISCSI),
            CrcAlgorithm::Crc32IsoHdlc => write!(f, "{}", NAME_CRC32_ISO_HDLC),
            CrcAlgorithm::Crc32Jamcrc => write!(f, "{}", NAME_CRC32_JAMCRC),
            CrcAlgorithm::Crc32Mef => write!(f, "{}", NAME_CRC32_MEF),
            CrcAlgorithm::Crc32Mpeg2 => write!(f, "{}", NAME_CRC32_MPEG_2),
            CrcAlgorithm::Crc32Xfer => write!(f, "{}", NAME_CRC32_XFER),
            CrcAlgorithm::Crc64GoIso => write!(f, "{}", NAME_CRC64_GO_ISO),
            CrcAlgorithm::Crc64Ms => write!(f, "{}", NAME_CRC64_MS),
            CrcAlgorithm::Crc64Nvme => write!(f, "{}", NAME_CRC64_NVME),
            CrcAlgorithm::Crc64Redis => write!(f, "{}", NAME_CRC64_REDIS),
            CrcAlgorithm::Crc64Xz => write!(f, "{}", NAME_CRC64_XZ),
            CrcAlgorithm::Crc64Ecma182 => write!(f, "{}", NAME_CRC64_ECMA_182),
            CrcAlgorithm::Crc64We => write!(f, "{}", NAME_CRC64_WE),
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
#[derive(Debug, Copy, Clone)]
pub(crate) enum Reflector<T> {
    NoReflector,
    ForwardReflector { smask: T },
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
/// Different processing strategies based on data length
pub(crate) enum DataChunkProcessor {
    From0To15,   // 0-15 bytes
    From16,      // exactly 16 bytes
    From17To31,  // 17-31 bytes
    From32To255, // 32-255 bytes
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
impl DataChunkProcessor {
    /// Select the appropriate processor based on data length
    pub fn for_length(len: usize) -> Self {
        match len {
            0..=15 => Self::From0To15,
            16 => Self::From16,
            17..=31 => Self::From17To31,
            32..=255 => Self::From32To255,
            _ => panic!("data length too large"),
        }
    }
}
