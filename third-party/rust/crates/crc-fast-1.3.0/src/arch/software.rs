// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! This module contains a software fallback for unsupported architectures.

#![cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]

use crate::consts::CRC_64_NVME;
use crate::structs::CrcParams;
use crate::CrcAlgorithm;
use crc::Table;

const RUST_CRC32_AIXM: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_AIXM);

const RUST_CRC32_AUTOSAR: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_AUTOSAR);

const RUST_CRC32_BASE91_D: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_BASE91_D);

const RUST_CRC32_BZIP2: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_BZIP2);

const RUST_CRC32_CD_ROM_EDC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_CD_ROM_EDC);

const RUST_CRC32_CKSUM: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_CKSUM);

const RUST_CRC32_ISCSI: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_ISCSI);

const RUST_CRC32_ISO_HDLC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_ISO_HDLC);

const RUST_CRC32_JAMCRC: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_JAMCRC);

const RUST_CRC32_MEF: crc::Crc<u32, Table<16>> = crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_MEF);

const RUST_CRC32_MPEG_2: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_MPEG_2);

const RUST_CRC32_XFER: crc::Crc<u32, Table<16>> =
    crc::Crc::<u32, Table<16>>::new(&crc::CRC_32_XFER);

const RUST_CRC64_ECMA_182: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_ECMA_182);

const RUST_CRC64_GO_ISO: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_GO_ISO);

const RUST_CRC64_MS: crc::Crc<u64, Table<16>> = crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_MS);

const RUST_CRC64_NVME: crc::Crc<u64, Table<16>> = crc::Crc::<u64, Table<16>>::new(&CRC_64_NVME);

const RUST_CRC64_REDIS: crc::Crc<u64, Table<16>> =
    crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_REDIS);

const RUST_CRC64_WE: crc::Crc<u64, Table<16>> = crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_WE);

const RUST_CRC64_XZ: crc::Crc<u64, Table<16>> = crc::Crc::<u64, Table<16>>::new(&crc::CRC_64_XZ);

// Dispatch function that handles the generic case
pub(crate) fn update(state: u64, data: &[u8], params: CrcParams) -> u64 {
    match params.width {
        32 => {
            let params = match params.algorithm {
                CrcAlgorithm::Crc32Aixm => RUST_CRC32_AIXM,
                CrcAlgorithm::Crc32Autosar => RUST_CRC32_AUTOSAR,
                CrcAlgorithm::Crc32Base91D => RUST_CRC32_BASE91_D,
                CrcAlgorithm::Crc32Bzip2 => RUST_CRC32_BZIP2,
                CrcAlgorithm::Crc32CdRomEdc => RUST_CRC32_CD_ROM_EDC,
                CrcAlgorithm::Crc32Cksum => RUST_CRC32_CKSUM,
                CrcAlgorithm::Crc32Iscsi => RUST_CRC32_ISCSI,
                CrcAlgorithm::Crc32IsoHdlc => RUST_CRC32_ISO_HDLC,
                CrcAlgorithm::Crc32Jamcrc => RUST_CRC32_JAMCRC,
                CrcAlgorithm::Crc32Mef => RUST_CRC32_MEF,
                CrcAlgorithm::Crc32Mpeg2 => RUST_CRC32_MPEG_2,
                CrcAlgorithm::Crc32Xfer => RUST_CRC32_XFER,
                _ => panic!("Invalid algorithm for u32 CRC"),
            };
            update_u32(state as u32, data, params) as u64
        }
        64 => {
            let params = match params.algorithm {
                CrcAlgorithm::Crc64Ecma182 => RUST_CRC64_ECMA_182,
                CrcAlgorithm::Crc64GoIso => RUST_CRC64_GO_ISO,
                CrcAlgorithm::Crc64Ms => RUST_CRC64_MS,
                CrcAlgorithm::Crc64Nvme => RUST_CRC64_NVME,
                CrcAlgorithm::Crc64Redis => RUST_CRC64_REDIS,
                CrcAlgorithm::Crc64We => RUST_CRC64_WE,
                CrcAlgorithm::Crc64Xz => RUST_CRC64_XZ,
                _ => panic!("Invalid algorithm for u64 CRC"),
            };
            update_u64(state, data, params)
        }
        _ => panic!("Unsupported CRC width: {}", params.width),
    }
}

// Specific implementation for u32
fn update_u32(state: u32, data: &[u8], params: crc::Crc<u32, Table<16>>) -> u32 {
    // apply REFIN if necessary
    let initial = if params.algorithm.refin {
        state.reverse_bits()
    } else {
        state
    };

    let mut digest = params.digest_with_initial(initial);
    digest.update(data);

    let checksum = digest.finalize();

    // remove XOR since this will be applied in the library Digest::finalize() step instead
    checksum ^ params.algorithm.xorout
}

// Specific implementation for u64
fn update_u64(state: u64, data: &[u8], params: crc::Crc<u64, Table<16>>) -> u64 {
    // apply REFIN if necessary
    let initial = if params.algorithm.refin {
        state.reverse_bits()
    } else {
        state
    };

    let mut digest = params.digest_with_initial(initial);
    digest.update(data);

    // remove XOR since this will be applied in the library Digest::finalize() step instead
    digest.finalize() ^ params.algorithm.xorout
}
