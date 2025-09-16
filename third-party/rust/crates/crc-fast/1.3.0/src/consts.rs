// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

#![allow(dead_code)]

use crc::Algorithm;

// Constants for common values with semantic meaning
pub(crate) const CRC_CHUNK_SIZE: usize = 16;
pub(crate) const CRC_HALF_CHUNK_SIZE: usize = 8;
pub(crate) const CRC_LARGE_CHUNK_THRESHOLD: usize = 256;

pub const NAME_CRC32_AIXM: &str = "CRC-32/AIXM";
pub const NAME_CRC32_AUTOSAR: &str = "CRC-32/AUTOSAR";
pub const NAME_CRC32_BASE91_D: &str = "CRC-32/BASE91-D";
pub const NAME_CRC32_BZIP2: &str = "CRC-32/BZIP2";
pub const NAME_CRC32_CD_ROM_EDC: &str = "CRC-32/CD-ROM-EDC";
pub const NAME_CRC32_CKSUM: &str = "CRC-32/CKSUM";
pub const NAME_CRC32_ISCSI: &str = "CRC-32/ISCSI";
pub const NAME_CRC32_ISO_HDLC: &str = "CRC-32/ISO-HDLC";
pub const NAME_CRC32_JAMCRC: &str = "CRC-32/JAMCRC";
pub const NAME_CRC32_MEF: &str = "CRC-32/MEF";
pub const NAME_CRC32_MPEG_2: &str = "CRC-32/MPEG-2";
pub const NAME_CRC32_XFER: &str = "CRC-32/XFER";

pub const NAME_CRC64_ECMA_182: &str = "CRC-64/ECMA-182";
pub const NAME_CRC64_GO_ISO: &str = "CRC-64/GO-ISO";
pub const NAME_CRC64_MS: &str = "CRC-64/MS";
pub const NAME_CRC64_NVME: &str = "CRC-64/NVME";
pub const NAME_CRC64_REDIS: &str = "CRC-64/REDIS";
pub const NAME_CRC64_WE: &str = "CRC-64/WE";
pub const NAME_CRC64_XZ: &str = "CRC-64/XZ";

// https://reveng.sourceforge.io/crc-catalogue/all.htm#crc.cat.crc-64-nvme
// width=64 poly=0xad93d23594c93659 init=0xffffffffffffffff refin=true refout=true xorout=0xffffffffffffffff check=0xae8b14860a799888 residue=0xf310303b2b6f6e42 name="CRC-64/NVME"
pub const CRC_64_NVME: Algorithm<u64> = Algorithm {
    width: 64,
    poly: 0xad93d23594c93659,
    init: 0xFFFFFFFFFFFFFFFF,
    refin: true,
    refout: true,
    xorout: 0xFFFFFFFFFFFFFFFF,
    check: 0xae8b14860a799888,
    residue: 0xf310303b2b6f6e42,
};

// for software fallbacks and testing
pub(crate) const RUST_CRC32_AIXM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_AIXM);

pub(crate) const RUST_CRC32_AUTOSAR: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_AUTOSAR);

pub(crate) const RUST_CRC32_BASE91_D: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_BASE91_D);

pub(crate) const RUST_CRC32_BZIP2: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_BZIP2);

pub(crate) const RUST_CRC32_CD_ROM_EDC: crc::Crc<u32> =
    crc::Crc::<u32>::new(&crc::CRC_32_CD_ROM_EDC);

pub(crate) const RUST_CRC32_CKSUM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

pub(crate) const RUST_CRC32_ISCSI: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);

pub(crate) const RUST_CRC32_ISO_HDLC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

pub(crate) const RUST_CRC32_JAMCRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_JAMCRC);

pub(crate) const RUST_CRC32_MEF: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_MEF);

pub(crate) const RUST_CRC32_MPEG_2: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_MPEG_2);

pub(crate) const RUST_CRC32_XFER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_XFER);

pub(crate) const RUST_CRC64_ECMA_182: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_ECMA_182);

pub(crate) const RUST_CRC64_GO_ISO: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_GO_ISO);

pub(crate) const RUST_CRC64_MS: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_MS);

pub(crate) const RUST_CRC64_NVME: crc::Crc<u64> = crc::Crc::<u64>::new(&CRC_64_NVME);

pub(crate) const RUST_CRC64_REDIS: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_REDIS);

pub(crate) const RUST_CRC64_WE: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_WE);

pub(crate) const RUST_CRC64_XZ: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
