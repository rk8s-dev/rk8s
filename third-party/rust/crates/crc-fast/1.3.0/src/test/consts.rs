// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

#![cfg(test)]
#![allow(dead_code)]

use crate::consts::CRC_64_NVME;
use crate::crc32::consts::{
    CRC32_AIXM, CRC32_AUTOSAR, CRC32_BASE91_D, CRC32_BZIP2, CRC32_CD_ROM_EDC, CRC32_CKSUM,
    CRC32_ISCSI, CRC32_ISO_HDLC, CRC32_JAMCRC, CRC32_MEF, CRC32_MPEG_2, CRC32_XFER,
};
use crate::crc64::consts::{
    CRC64_ECMA_182, CRC64_GO_ISO, CRC64_MS, CRC64_NVME, CRC64_REDIS, CRC64_WE, CRC64_XZ,
};
use crate::test::enums::*;
use crate::test::structs::*;
use crc::Table;

pub const TEST_CHECK_STRING: &[u8] = b"123456789";

pub const TEST_256_BYTES_STRING: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456";

pub const TEST_255_BYTES_STRING: &[u8] = b"123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345";

pub(crate) const RUST_CRC32_AIXM: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_AIXM);

pub(crate) const RUST_CRC32_AUTOSAR: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_AUTOSAR);

pub(crate) const RUST_CRC32_BASE91_D: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_BASE91_D);

pub(crate) const RUST_CRC32_BZIP2: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_BZIP2);

pub(crate) const RUST_CRC32_CD_ROM_EDC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_CD_ROM_EDC);

pub(crate) const RUST_CRC32_CKSUM: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_CKSUM);

pub(crate) const RUST_CRC32_ISCSI: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_ISCSI);

pub(crate) const RUST_CRC32_ISO_HDLC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_ISO_HDLC);

pub(crate) const RUST_CRC32_JAMCRC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_JAMCRC);

pub(crate) const RUST_CRC32_MEF: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_MEF);

pub(crate) const RUST_CRC32_MPEG_2: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_MPEG_2);

pub(crate) const RUST_CRC32_XFER: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_XFER);

pub(crate) const RUST_CRC64_ECMA_182: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_ECMA_182);

pub(crate) const RUST_CRC64_GO_ISO: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_GO_ISO);

pub(crate) const RUST_CRC64_MS: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_MS);

pub(crate) const RUST_CRC64_NVME: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&CRC_64_NVME);

pub(crate) const RUST_CRC64_REDIS: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_REDIS);

pub(crate) const RUST_CRC64_WE: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_WE);

pub(crate) const RUST_CRC64_XZ: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_XZ);

pub(crate) const TEST_CRC64_ECMA_182: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_ECMA_182,
    reference_impl: &RUST_CRC64_ECMA_182,
};

pub(crate) const TEST_CRC64_GO_ISO: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_GO_ISO,
    reference_impl: &RUST_CRC64_GO_ISO,
};

pub(crate) const TEST_CRC64_MS: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_MS,
    reference_impl: &RUST_CRC64_MS,
};

pub(crate) const TEST_CRC64_NVME: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_NVME,
    reference_impl: &RUST_CRC64_NVME,
};

pub(crate) const TEST_CRC64_REDIS: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_REDIS,
    reference_impl: &RUST_CRC64_REDIS,
};

pub(crate) const TEST_CRC64_WE: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_WE,
    reference_impl: &RUST_CRC64_WE,
};

pub(crate) const TEST_CRC64_XZ: Crc64TestConfig = Crc64TestConfig {
    params: CRC64_XZ,
    reference_impl: &RUST_CRC64_XZ,
};

pub(crate) const TEST_CRC32_AIXM: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_AIXM,
    reference_impl: &RUST_CRC32_AIXM,
};

pub(crate) const TEST_CRC32_AUTOSAR: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_AUTOSAR,
    reference_impl: &RUST_CRC32_AUTOSAR,
};

pub(crate) const TEST_CRC32_BASE91_D: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_BASE91_D,
    reference_impl: &RUST_CRC32_BASE91_D,
};

pub(crate) const TEST_CRC32_BZIP2: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_BZIP2,
    reference_impl: &RUST_CRC32_BZIP2,
};

pub(crate) const TEST_CRC32_CD_ROM_EDC: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_CD_ROM_EDC,
    reference_impl: &RUST_CRC32_CD_ROM_EDC,
};

pub(crate) const TEST_CRC32_CKSUM: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_CKSUM,
    reference_impl: &RUST_CRC32_CKSUM,
};

pub(crate) const TEST_CRC32_ISCSI: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_ISCSI,
    reference_impl: &RUST_CRC32_ISCSI,
};

pub(crate) const TEST_CRC32_ISO_HDLC: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_ISO_HDLC,
    reference_impl: &RUST_CRC32_ISO_HDLC,
};

pub(crate) const TEST_CRC32_JAMCRC: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_JAMCRC,
    reference_impl: &RUST_CRC32_JAMCRC,
};

pub(crate) const TEST_CRC32_MEF: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_MEF,
    reference_impl: &RUST_CRC32_MEF,
};

pub(crate) const TEST_CRC32_MPEG_2: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_MPEG_2,
    reference_impl: &RUST_CRC32_MPEG_2,
};

pub(crate) const TEST_CRC32_XFER: Crc32TestConfig = Crc32TestConfig {
    params: CRC32_XFER,
    reference_impl: &RUST_CRC32_XFER,
};

pub(crate) const TEST_ALL_CONFIGS: &[AnyCrcTestConfig] = &[
    AnyCrcTestConfig::CRC32(&TEST_CRC32_AIXM),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_AUTOSAR),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_BASE91_D),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_BZIP2),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_CD_ROM_EDC),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_CKSUM),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_ISCSI),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_ISO_HDLC),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_JAMCRC),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_MEF),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_MPEG_2),
    AnyCrcTestConfig::CRC32(&TEST_CRC32_XFER),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_ECMA_182),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_GO_ISO),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_MS),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_NVME),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_REDIS),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_WE),
    AnyCrcTestConfig::CRC64(&TEST_CRC64_XZ),
];
