// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! This module provides the main entry point for the SIMD CRC calculation.
//!
//! It dispatches to the appropriate architecture-specific implementation
//! based on the target architecture.

#[cfg(target_arch = "aarch64")]
use std::arch::is_aarch64_feature_detected;

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
use crate::algorithm;

use crate::structs::CrcParams;

#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
use crate::structs::{Width32, Width64};

#[cfg(target_arch = "aarch64")]
use aarch64::AArch64Ops;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use x86::X86Ops;

//#[rustversion::since(1.89)]
#[cfg(all(target_arch = "x86_64", feature = "vpclmulqdq"))]
use vpclmulqdq::Vpclmulqdq512Ops;

mod aarch64;
mod software;
mod vpclmulqdq;
mod x86;

/// Main entry point that dispatches to the appropriate architecture
///
///
/// # Safety
/// May use native CPU features
#[inline]
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "aes")]
pub(crate) unsafe fn update(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    let ops = AArch64Ops;

    match params.width {
        64 => algorithm::update::<AArch64Ops, Width64>(state, bytes, params, &ops),
        32 => algorithm::update::<AArch64Ops, Width32>(state as u32, bytes, params, &ops) as u64,
        _ => panic!("Unsupported CRC width: {}", params.width),
    }
}

//#[rustversion::before(1.89)]
#[inline]
#[cfg(all(
    not(feature = "vpclmulqdq"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
#[target_feature(enable = "ssse3,sse4.1,pclmulqdq")]
pub(crate) unsafe fn update(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    update_x86_sse(state, bytes, params)
}

//#[rustversion::since(1.89)]
#[inline]
#[cfg(all(feature = "vpclmulqdq", target_arch = "x86"))]
#[target_feature(enable = "ssse3,sse4.1,pclmulqdq")]
pub(crate) unsafe fn update(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    update_x86_sse(state, bytes, params)
}

//#[rustversion::since(1.89)]
#[inline]
#[cfg(all(feature = "vpclmulqdq", target_arch = "x86_64"))]
#[target_feature(enable = "ssse3,sse4.1,pclmulqdq")]
pub(crate) unsafe fn update(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    use std::arch::is_x86_feature_detected;

    if bytes.len() >= 256
        && is_x86_feature_detected!("vpclmulqdq")
        && is_x86_feature_detected!("avx512f")
        && is_x86_feature_detected!("avx512vl")
    {
        let ops = Vpclmulqdq512Ops::new();

        return match params.width {
            64 => algorithm::update::<Vpclmulqdq512Ops, Width64>(state, bytes, params, &ops),
            32 => algorithm::update::<Vpclmulqdq512Ops, Width32>(state as u32, bytes, params, &ops)
                as u64,
            _ => panic!("Unsupported CRC width: {}", params.width),
        };
    }

    // fallback to the standard x86 SSE implementation
    update_x86_sse(state, bytes, params)
}

#[inline]
#[cfg(all(
    not(target_arch = "x86"),
    not(target_arch = "x86_64"),
    not(target_arch = "aarch64")
))]
pub(crate) unsafe fn update(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    software::update(state, bytes, params)
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "ssse3,sse4.1,pclmulqdq")]
unsafe fn update_x86_sse(state: u64, bytes: &[u8], params: CrcParams) -> u64 {
    let ops = X86Ops;

    match params.width {
        64 => algorithm::update::<X86Ops, Width64>(state, bytes, params, &ops),
        32 => algorithm::update::<X86Ops, Width32>(state as u32, bytes, params, &ops) as u64,
        _ => panic!("Unsupported CRC width: {}", params.width),
    }
}

//#[rustversion::before(1.89)]
#[cfg(not(feature = "vpclmulqdq"))]
pub fn get_target() -> String {
    #[cfg(target_arch = "aarch64")]
    {
        if is_aarch64_feature_detected!("sha3") {
            return "aarch64-neon-eor3-pclmulqdq".to_string();
        }

        "aarch64-neon-pclmulqdq".to_string()
    }

    #[allow(unreachable_code)]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return "x86-sse-pclmulqdq".to_string();

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    return "software-fallback-tables".to_string();
}

//#[rustversion::since(1.89)]
#[cfg(feature = "vpclmulqdq")]
pub fn get_target() -> String {
    #[cfg(target_arch = "aarch64")]
    {
        if is_aarch64_feature_detected!("sha3") {
            return "aarch64-neon-eor3-pclmulqdq".to_string();
        }

        "aarch64-neon-pclmulqdq".to_string()
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("vpclmulqdq")
            && is_x86_feature_detected!("avx512f")
            && is_x86_feature_detected!("avx512vl")
        {
            return "x86_64-avx512-vpclmulqdq".to_string();
        }

        if is_x86_feature_detected!("avx2") {
            return "x86_64-avx2-pclmulqdq".to_string();
        }
    }

    #[allow(unreachable_code)]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return "x86-sse-pclmulqdq".to_string();

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    return "software-fallback-tables".to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc32::consts::CRC32_BZIP2;
    use crate::crc64::consts::CRC64_NVME;
    use crate::test::consts::{TEST_256_BYTES_STRING, TEST_ALL_CONFIGS, TEST_CHECK_STRING};
    use crate::test::create_aligned_data;
    use crate::test::enums::AnyCrcTestConfig;
    use rand::{rng, Rng};

    #[test]
    fn test_check_value() {
        for config in TEST_ALL_CONFIGS {
            // direct update() call, which needs XOROUT applied
            let actual = unsafe {
                update(config.get_init(), TEST_CHECK_STRING, *config.get_params())
                    ^ config.get_xorout()
            };

            assert_eq!(
                actual,
                config.get_check(),
                "Mismatch CRC, {}, expected {:#x}, got {:#x}",
                config.get_name(),
                config.get_check(),
                actual
            );
        }
    }

    #[test]
    fn test_256_string() {
        for config in TEST_ALL_CONFIGS {
            let actual = unsafe {
                update(
                    config.get_init(),
                    &*create_aligned_data(TEST_256_BYTES_STRING),
                    *config.get_params(),
                ) ^ config.get_xorout()
            };

            assert_eq!(
                actual,
                config.checksum_with_reference(TEST_256_BYTES_STRING),
                "Mismatch CRC, {}, expected {:#x}, got {:#x}",
                config.get_name(),
                config.get_check(),
                actual
            );
        }
    }

    #[test]
    fn test_512_string() {
        let test_string = b"12345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456";

        for config in TEST_ALL_CONFIGS {
            let actual = unsafe {
                update(
                    config.get_init(),
                    &*create_aligned_data(test_string),
                    *config.get_params(),
                ) ^ config.get_xorout()
            };

            assert_eq!(
                actual,
                config.checksum_with_reference(test_string),
                "Mismatch CRC, {}, expected {:#x}, got {:#x}",
                config.get_name(),
                config.get_check(),
                actual
            );
        }
    }

    #[test]
    fn test_1024_string() {
        let test_string = b"1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345612345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456";

        for config in TEST_ALL_CONFIGS {
            let actual = unsafe {
                update(
                    config.get_init(),
                    &*create_aligned_data(test_string),
                    *config.get_params(),
                ) ^ config.get_xorout()
            };

            assert_eq!(
                actual,
                config.checksum_with_reference(test_string),
                "Mismatch CRC, {}, expected {:#x}, got {:#x}",
                config.get_name(),
                config.get_check(),
                actual
            );
        }
    }

    // CRC-64/NVME is a special flower in that Rust's crc library doesn't support it yet, so we have
    // tested values to check against.
    #[test]
    fn test_crc64_nvme_standard_vectors() {
        static CASES: &[(&[u8], u64)] = &[
            // from our own internal tests, since the Check value in the NVM Express® NVM Command
            // Set Specification (Revision 1.0d, December 2023) is incorrect
            // (Section 5.2.1.3.4, Figure 120, page 83).
            (b"123456789", 0xae8b14860a799888),

            // from the NVM Express® NVM Command Set Specification (Revision 1.0d, December 2023),
            // Section 5.2.1.3.5, Figure 122, page 84.
            // https://nvmexpress.org/wp-content/uploads/NVM-Express-NVM-Command-Set-Specification-1.0d-2023.12.28-Ratified.pdf
            // and the Linux kernel
            // https://github.com/torvalds/linux/blob/f3813f4b287e480b1fcd62ca798d8556644b8278/crypto/testmgr.h#L3685-L3695
            (&[0; 4096], 0x6482d367eb22b64e),
            (&[255; 4096], 0xc0ddba7302eca3ac),

            // custom values
            (TEST_256_BYTES_STRING, 0xabdb9e6c30937916),
            (b"", 0),
            (b"@", 0x2808afa9582aa47),
            (b"1\x97", 0xb4af0ae0feb08e0f),
            (b"M\"\xdf", 0x85d7cd041a2a8a5d),
            (b"l\xcd\x13\xd7", 0x1860820ea79b0fa3),
            (&[0; 32], 0xcf3473434d4ecf3b),
            (&[255; 32], 0xa0a06974c34d63c4),
            (b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C\x0D\x0E\x0F\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1A\x1B\x1C\x1D\x1E\x1F", 0xb9d9d4a8492cbd7f),
            (&[0; 1024], 0x691bb2b09be5498a),
            (b"hello, world!", 0xf8046e40c403f1d0),
        ];

        for (input, expected) in CASES {
            unsafe {
                let actual = update(CRC64_NVME.init, input, CRC64_NVME) ^ CRC64_NVME.xorout;

                assert_eq!(
                    actual, *expected,
                    "Mismatch CRC, expected {:#x}, got {:#x}, input: {:?}",
                    expected, actual, input
                );
            }
        }
    }

    /// Test the "crc32" variant used in PHP's hash() function, which is different from the
    /// crc32() function. It's really just CRC-32/BZIP2 with the output byte-reversed to little
    /// endian.
    ///
    /// https://www.php.net/manual/en/function.hash-file.php#104836
    #[test]
    fn test_crc32_php_standard_vectors() {
        static CASES: &[(&[u8], u64)] = &[
            (b"123456789", 0x181989fc),
            (&[0; 4096], 0xe3380088),
            (&[255; 4096], 0x8f2ae650),
            (b"hello, world!", 0x5eacce7),
        ];

        for (input, expected) in CASES {
            let bzip2_crc = unsafe {
                (update(CRC32_BZIP2.init, input, CRC32_BZIP2) ^ CRC32_BZIP2.xorout) as u32
            };

            // PHP reverses the byte order of the CRC for some reason
            let actual = bzip2_crc.swap_bytes();

            assert_eq!(
                actual, *expected as u32,
                "Mismatch CRC, expected {:#x}, got {:#x}, input: {:?}",
                expected, actual, input
            );
        }
    }

    #[test]
    fn test_small_lengths_all() {
        // Test each CRC-64 variant
        for config in TEST_ALL_CONFIGS {
            // Test each length from 0 to 255
            for len in 0..=255 {
                test_length(len, config);
            }
        }
    }

    #[test]
    fn test_medium_lengths() {
        // Test each CRC-64 variant
        for config in TEST_ALL_CONFIGS {
            // Test each length from 256 to 1024, which should fold and include handling remainders
            for len in 256..=1024 {
                test_length(len, config);
            }
        }
    }

    #[test]
    fn test_large_lengths() {
        // Test each CRC-64 variant
        for config in TEST_ALL_CONFIGS {
            // Test ~1 MiB just before, at, and just after the folding boundaries
            for len in 1048575..=1048577 {
                test_length(len, config);
            }
        }
    }

    fn test_length(length: usize, config: &AnyCrcTestConfig) {
        let mut data = vec![0u8; length];
        rng().fill(&mut data[..]);

        // Calculate expected CRC using the reference implementation
        let expected = config.checksum_with_reference(&data);

        // direct update() call, which needs XOROUT applied
        let actual =
            unsafe { update(config.get_init(), &data, *config.get_params()) ^ config.get_xorout() };

        assert_eq!(
            actual,
            expected,
            "\nFailed for {} with length {}\nGot: {:016x}\nExpected: {:016x}",
            config.get_name(),
            length,
            actual,
            expected
        );
    }
}
