// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! `crc-fast`
//! ===========
//!
//! Hardware-accelerated CRC calculation for
//! [all known CRC-32 and CRC-64 variants](https://reveng.sourceforge.io/crc-catalogue/all.htm)
//! using SIMD intrinsics which can exceed 100GiB/s for CRC-32 and 50GiB/s for CRC-64 on modern
//! systems.
//!
//! # Other languages
//!
//! Supplies a C-compatible shared library for use with other non-Rust languages. See
//! [PHP extension](https://github.com/awesomized/crc-fast-php-ext) example.
//!
//! # Background
//!
//! The implementation is based on Intel's
//! [Fast CRC Computation for Generic Polynomials Using PCLMULQDQ Instruction](https://web.archive.org/web/20131224125630/https://www.intel.com/content/dam/www/public/us/en/documents/white-papers/fast-crc-computation-generic-polynomials-pclmulqdq-paper.pdf),
//! white paper though it folds 8-at-a-time, like other modern implementations, rather than the
//! 4-at-a-time as in Intel's paper.
//!
//! Works on `aarch64`, `x86_64`, and `x86` architectures, and is hardware-accelerated and optimized
//! for each architecture.
//!
//! Inspired by [`crc32fast`](https://crates.io/crates/crc32fast),
//! [`crc64fast`](https://crates.io/crates/crc64fast),
//! and [`crc64fast-nvme`](https://crates.io/crates/crc64fast-nvme), each of which only accelerates
//! a single, different CRC variant, and all of them were "reflected" variants.
//!
//! In contrast, this library accelerates _every known variant_ (and should accelerate any future
//! variants without changes), including all the "non-reflected" variants.
//!
//! # Usage
//!
//! ## Digest
//!
//! Implements the [digest::DynDigest](https://docs.rs/digest/latest/digest/trait.DynDigest.html)
//! trait for easier integration with existing code.
//!
//! ```rust
//! use crc_fast::{Digest, CrcAlgorithm::Crc32IsoHdlc};
//!
//! let mut digest = Digest::new(Crc32IsoHdlc);
//! digest.update(b"1234");
//! digest.update(b"56789");
//! let checksum = digest.finalize();
//!
//! assert_eq!(checksum, 0xcbf43926);
//! ```
//!
//! ## Digest Write
//!
//! Implements the [std::io::Write](https://doc.rust-lang.org/std/io/trait.Write.html) trait for
//! easier integration with existing code.
//!
//! ```no_run
//! use std::env;
//! use std::fs::File;
//! use crc_fast::{Digest, CrcAlgorithm::Crc32IsoHdlc};
//!
//! // for example/test purposes only, use your own file path
//! let file_path = env::current_dir().expect("missing working dir").join("crc-check.txt");
//! let file_on_disk = file_path.to_str().unwrap();
//!
//! // actual usage
//! let mut digest = Digest::new(Crc32IsoHdlc);
//! let mut file = File::open(file_on_disk).unwrap();
//! std::io::copy(&mut file, &mut digest).unwrap();
//! let checksum = digest.finalize();
//!
//! assert_eq!(checksum, 0xcbf43926);
//! ```
//! ## checksum
//!```rust
//! use crc_fast::{checksum, CrcAlgorithm::Crc32IsoHdlc};
//!
//! let checksum = checksum(Crc32IsoHdlc, b"123456789");
//!
//! assert_eq!(checksum, 0xcbf43926);
//! ```
//!
//! ## checksum_combine
//!```rust
//! use crc_fast::{checksum, checksum_combine, CrcAlgorithm::Crc32IsoHdlc};
//!
//! let checksum_1 = checksum(Crc32IsoHdlc, b"1234");
//! let checksum_2 = checksum(Crc32IsoHdlc, b"56789");
//! let checksum = checksum_combine(Crc32IsoHdlc, checksum_1, checksum_2, 5);
//!
//! assert_eq!(checksum, 0xcbf43926);
//! ```
//!
//! ## checksum_file
//!```rust
//! use std::env;
//! use crc_fast::{checksum_file, CrcAlgorithm::Crc32IsoHdlc};
//!
//! // for example/test purposes only, use your own file path
//! let file_path = env::current_dir().expect("missing working dir").join("crc-check.txt");
//! let file_on_disk = file_path.to_str().unwrap();
//!
//! let checksum = checksum_file(Crc32IsoHdlc, file_on_disk, None);
//!
//! assert_eq!(checksum.unwrap(), 0xcbf43926);
//! ```

use crate::crc32::consts::{
    CRC32_AIXM, CRC32_AUTOSAR, CRC32_BASE91_D, CRC32_BZIP2, CRC32_CD_ROM_EDC, CRC32_CKSUM,
    CRC32_ISCSI, CRC32_ISO_HDLC, CRC32_JAMCRC, CRC32_MEF, CRC32_MPEG_2, CRC32_XFER,
};

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
use crate::crc32::fusion;

use crate::crc64::consts::{
    CRC64_ECMA_182, CRC64_GO_ISO, CRC64_MS, CRC64_NVME, CRC64_REDIS, CRC64_WE, CRC64_XZ,
};
use crate::structs::{Calculator, CrcParams};
use crate::traits::CrcCalculator;
use digest::{DynDigest, InvalidBufferSize};
use std::fs::File;
use std::io::{Read, Write};

mod algorithm;
mod arch;
mod combine;
mod consts;
mod crc32;
mod crc64;
mod enums;
mod ffi;
mod generate;
mod structs;
mod test;
mod traits;

/// Supported CRC-32 and CRC-64 variants
#[derive(Debug, Clone, Copy)]
pub enum CrcAlgorithm {
    Crc32Aixm,
    Crc32Autosar,
    Crc32Base91D,
    Crc32Bzip2,
    Crc32CdRomEdc,
    Crc32Cksum,
    Crc32Iscsi,
    Crc32IsoHdlc,
    Crc32Jamcrc,
    Crc32Mef,
    Crc32Mpeg2,
    Crc32Xfer,
    Crc64Ecma182,
    Crc64GoIso,
    Crc64Ms,
    Crc64Nvme,
    Crc64Redis,
    Crc64We,
    Crc64Xz,
}

/// Type alias for a function pointer that represents a CRC calculation function.
///
/// The function takes the following parameters:
/// - `state`: The current state of the CRC computation.
/// - `data`: A slice of bytes to be processed.
/// - `params`: The parameters for the CRC computation, such as polynomial, initial value, etc.
///
/// The function returns the updated state after processing the data.
type CalculatorFn = fn(
    u64,       // state
    &[u8],     // data
    CrcParams, // CRC implementation parameters
) -> u64;

/// Represents a CRC Digest, which is used to compute CRC checksums.
///
/// The `Digest` struct maintains the state of the CRC computation, including
/// the current state, the amount of data processed, the CRC parameters, and
/// the calculator function used to perform the CRC calculation.
#[derive(Copy, Clone, Debug)]
pub struct Digest {
    /// The current state of the CRC computation.
    state: u64,

    /// The total amount of data processed so far.
    amount: u64,

    /// The parameters for the CRC computation, such as polynomial, initial value, etc.
    params: CrcParams,

    /// The function used to perform the CRC calculation.
    calculator: CalculatorFn,
}

impl DynDigest for Digest {
    #[inline(always)]
    fn update(&mut self, data: &[u8]) {
        self.update(data);
    }

    #[inline(always)]
    fn finalize_into(self, buf: &mut [u8]) -> Result<(), InvalidBufferSize> {
        if buf.len() != self.output_size() {
            return Err(InvalidBufferSize);
        }

        let result = self.finalize();
        let bytes = if self.output_size() == 4 {
            result.to_be_bytes()[4..].to_vec() // Take last 4 bytes for 32-bit CRC
        } else {
            result.to_be_bytes().to_vec() // Use all 8 bytes for 64-bit CRC
        };
        buf.copy_from_slice(&bytes[..self.output_size()]);

        Ok(())
    }

    #[inline(always)]
    fn finalize_into_reset(&mut self, out: &mut [u8]) -> Result<(), InvalidBufferSize> {
        if out.len() != self.output_size() {
            return Err(InvalidBufferSize);
        }
        let result = self.finalize();
        self.reset();
        let bytes = if self.output_size() == 4 {
            result.to_be_bytes()[4..].to_vec() // Take last 4 bytes for 32-bit CRC
        } else {
            result.to_be_bytes().to_vec() // Use all 8 bytes for 64-bit CRC
        };
        out.copy_from_slice(&bytes[..self.output_size()]);
        Ok(())
    }

    #[inline(always)]
    fn reset(&mut self) {
        self.reset();
    }

    #[inline(always)]
    fn output_size(&self) -> usize {
        self.params.width as usize / 8
    }

    fn box_clone(&self) -> Box<dyn DynDigest> {
        Box::new(*self)
    }
}

impl Digest {
    /// Creates a new `Digest` instance for the specified CRC algorithm.
    #[inline(always)]
    pub fn new(algorithm: CrcAlgorithm) -> Self {
        let (calculator, params) = get_calculator_params(algorithm);

        Self {
            state: params.init,
            amount: 0,
            params,
            calculator,
        }
    }

    /// Updates the CRC state with the given data.
    #[inline(always)]
    pub fn update(&mut self, data: &[u8]) {
        self.state = (self.calculator)(self.state, data, self.params);
        self.amount += data.len() as u64;
    }

    /// Finalizes the CRC computation and returns the result.
    #[inline(always)]
    pub fn finalize(&self) -> u64 {
        self.state ^ self.params.xorout
    }

    /// Finalizes the CRC computation, resets the state, and returns the result.
    #[inline(always)]
    pub fn finalize_reset(&mut self) -> u64 {
        let result = self.finalize();
        self.reset();

        result
    }

    /// Resets the CRC state to its initial value.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.state = self.params.init;
        self.amount = 0;
    }

    /// Combines the CRC state with a second `Digest` instance.
    #[inline(always)]
    pub fn combine(&mut self, other: &Self) {
        self.amount += other.amount;
        let other_crc = other.finalize();

        // note the xorout for the input, since it's already been applied so it has to be removed,
        // and then re-adding it on the final output
        self.state = combine::checksums(
            self.state ^ self.params.xorout,
            other_crc,
            other.amount,
            self.params,
        ) ^ self.params.xorout;
    }

    /// Gets the amount of data processed so far
    #[inline(always)]
    pub fn get_amount(&self) -> u64 {
        self.amount
    }
}

impl Write for Digest {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.update(buf);
        Ok(buf.len())
    }

    #[inline(always)]
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        let len: usize = bufs
            .iter()
            .map(|buf| {
                self.update(buf);
                buf.len()
            })
            .sum();

        Ok(len)
    }

    #[inline(always)]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    #[inline(always)]
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.update(buf);

        Ok(())
    }
}

/// Computes the CRC checksum for the given data using the specified algorithm.
///
///```rust
/// use crc_fast::{checksum, CrcAlgorithm::Crc32IsoHdlc};
/// let checksum = checksum(Crc32IsoHdlc, b"123456789");
///
/// assert_eq!(checksum, 0xcbf43926);
/// ```
#[inline(always)]
pub fn checksum(algorithm: CrcAlgorithm, buf: &[u8]) -> u64 {
    let (calculator, params) = get_calculator_params(algorithm);

    calculator(params.init, buf, params) ^ params.xorout
}

/// Computes the CRC checksum for the given file using the specified algorithm.
///
/// Appears to be much faster (~2X) than using Writer and io::*, at least on Apple M2 Ultra
///
/// # Errors
///
/// This function will return an error if the file cannot be read.
///
/// # Examples
/// ### checksum_file
///```no_run
/// use std::env;
/// use crc_fast::{checksum_file, CrcAlgorithm::Crc32IsoHdlc};
///
/// // for example/test purposes only, use your own file path
/// let file_path = env::current_dir().expect("missing working dir").join("crc-check.txt");
/// let file_on_disk = file_path.to_str().unwrap();
///
/// let checksum = checksum_file(Crc32IsoHdlc, file_on_disk, None);
///
/// assert_eq!(checksum.unwrap(), 0xcbf43926);
/// ```
#[inline(always)]
pub fn checksum_file(
    algorithm: CrcAlgorithm,
    path: &str,
    chunk_size: Option<usize>,
) -> Result<u64, std::io::Error> {
    let mut digest = Digest::new(algorithm);
    let mut file = File::open(path)?;

    // 512KiB KiB was fastest in my benchmarks on an Apple M2 Ultra
    //
    // 4KiB ~7GiB/s
    // 64KiB ~22 GiB/s
    // 512KiB ~24 GiB/s
    let chunk_size = chunk_size.unwrap_or(524288);

    let mut buf = vec![0; chunk_size];

    while let Ok(n) = file.read(&mut buf) {
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }

    Ok(digest.finalize())
}

/// Combines two CRC checksums using the specified algorithm.
///
/// # Examples
///```rust
/// use crc_fast::{checksum, checksum_combine, CrcAlgorithm::Crc32IsoHdlc};
///
/// let checksum_1 = checksum(Crc32IsoHdlc, b"1234");
/// let checksum_2 = checksum(Crc32IsoHdlc, b"56789");
/// let checksum = checksum_combine(Crc32IsoHdlc, checksum_1, checksum_2, 5);
///
/// assert_eq!(checksum, 0xcbf43926);
/// ```
#[inline(always)]
pub fn checksum_combine(
    algorithm: CrcAlgorithm,
    checksum1: u64,
    checksum2: u64,
    checksum2_len: u64,
) -> u64 {
    let params = get_calculator_params(algorithm).1;

    combine::checksums(checksum1, checksum2, checksum2_len, params)
}

/// Returns the target used to calculate the CRC checksum for the specified algorithm.
///
/// These strings are informational only, not stable, and shouldn't be relied on to match across
/// versions.
///
/// # Examples
///```rust
/// use crc_fast::{get_calculator_target, CrcAlgorithm::Crc32IsoHdlc};
///
/// let target = get_calculator_target(Crc32IsoHdlc);
/// ```
pub fn get_calculator_target(_algorithm: CrcAlgorithm) -> String {
    arch::get_target()
}

/// Returns the calculator function and parameters for the specified CRC algorithm.
#[inline(always)]
fn get_calculator_params(algorithm: CrcAlgorithm) -> (CalculatorFn, CrcParams) {
    match algorithm {
        CrcAlgorithm::Crc32Aixm => (Calculator::calculate as CalculatorFn, CRC32_AIXM),
        CrcAlgorithm::Crc32Autosar => (Calculator::calculate as CalculatorFn, CRC32_AUTOSAR),
        CrcAlgorithm::Crc32Base91D => (Calculator::calculate as CalculatorFn, CRC32_BASE91_D),
        CrcAlgorithm::Crc32Bzip2 => (Calculator::calculate as CalculatorFn, CRC32_BZIP2),
        CrcAlgorithm::Crc32CdRomEdc => (Calculator::calculate as CalculatorFn, CRC32_CD_ROM_EDC),
        CrcAlgorithm::Crc32Cksum => (Calculator::calculate as CalculatorFn, CRC32_CKSUM),
        CrcAlgorithm::Crc32Iscsi => (crc32_iscsi_calculator as CalculatorFn, CRC32_ISCSI),
        CrcAlgorithm::Crc32IsoHdlc => (crc32_iso_hdlc_calculator as CalculatorFn, CRC32_ISO_HDLC),
        CrcAlgorithm::Crc32Jamcrc => (Calculator::calculate as CalculatorFn, CRC32_JAMCRC),
        CrcAlgorithm::Crc32Mef => (Calculator::calculate as CalculatorFn, CRC32_MEF),
        CrcAlgorithm::Crc32Mpeg2 => (Calculator::calculate as CalculatorFn, CRC32_MPEG_2),
        CrcAlgorithm::Crc32Xfer => (Calculator::calculate as CalculatorFn, CRC32_XFER),
        CrcAlgorithm::Crc64Ecma182 => (Calculator::calculate as CalculatorFn, CRC64_ECMA_182),
        CrcAlgorithm::Crc64GoIso => (Calculator::calculate as CalculatorFn, CRC64_GO_ISO),
        CrcAlgorithm::Crc64Ms => (Calculator::calculate as CalculatorFn, CRC64_MS),
        CrcAlgorithm::Crc64Nvme => (Calculator::calculate as CalculatorFn, CRC64_NVME),
        CrcAlgorithm::Crc64Redis => (Calculator::calculate as CalculatorFn, CRC64_REDIS),
        CrcAlgorithm::Crc64We => (Calculator::calculate as CalculatorFn, CRC64_WE),
        CrcAlgorithm::Crc64Xz => (Calculator::calculate as CalculatorFn, CRC64_XZ),
    }
}

/// Calculates the CRC-32/ISCSI ("crc32c" in many, but not all, implementations) checksum.
///
/// Because both aarch64 and x86 have native hardware support for CRC-32/ISCSI, we can use
/// fusion techniques to accelerate the calculation beyond what SIMD can do alone.
#[inline(always)]
fn crc32_iscsi_calculator(state: u64, data: &[u8], _params: CrcParams) -> u64 {
    // both aarch64 and x86 have native CRC-32/ISCSI support, so we can use fusion
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    return fusion::crc32_iscsi(state as u32, data) as u64;

    #[cfg(all(not(target_arch = "aarch64"), not(target_arch = "x86_64")))]
    // fallback to traditional calculation if not aarch64 or x86_64
    Calculator::calculate(state, data, _params)
}

/// Calculates the CRC-32/ISO-HDLC ("crc32" in many, but not all, implementations) checksum.
///
/// Because aarch64 has native hardware support for CRC-32/ISO-HDLC, we can use fusion techniques
/// to accelerate the calculation beyond what SIMD can do alone. x86 does not have native support,
/// so we use the traditional calculation.
#[inline(always)]
fn crc32_iso_hdlc_calculator(state: u64, data: &[u8], _params: CrcParams) -> u64 {
    // aarch64 CPUs have native CRC-32/ISO-HDLC support, so we can use the fusion implementation
    #[cfg(target_arch = "aarch64")]
    return fusion::crc32_iso_hdlc(state as u32, data) as u64;

    // x86 CPUs don't have native CRC-32/ISO-HDLC support, so there's no fusion to be had, use
    // traditional calculation
    #[cfg(not(target_arch = "aarch64"))]
    Calculator::calculate(state, data, _params)
}

#[cfg(test)]
mod lib {
    #![allow(unused)]

    use super::*;
    use crate::test::consts::{TEST_ALL_CONFIGS, TEST_CHECK_STRING};
    use crate::test::enums::AnyCrcTestConfig;
    use cbindgen::Language::{Cxx, C};
    use cbindgen::Style::Both;
    use rand::{rng, Rng};
    use std::fs::{read, write};

    #[test]
    fn test_checksum_check() {
        for config in TEST_ALL_CONFIGS {
            assert_eq!(
                checksum(config.get_algorithm(), TEST_CHECK_STRING),
                config.get_check()
            );
        }
    }

    #[test]
    fn test_checksum_reference() {
        for config in TEST_ALL_CONFIGS {
            assert_eq!(
                checksum(config.get_algorithm(), TEST_CHECK_STRING),
                config.checksum_with_reference(TEST_CHECK_STRING)
            );
        }
    }

    #[test]
    fn test_digest_updates_check() {
        for config in TEST_ALL_CONFIGS {
            let mut digest = Digest::new(config.get_algorithm());
            digest.update(b"123");
            digest.update(b"456");
            digest.update(b"789");
            let result = digest.finalize();

            assert_eq!(result, config.get_check());
        }
    }

    #[test]
    fn test_small_all_lengths() {
        for config in TEST_ALL_CONFIGS {
            // Test each length from 1 to 255
            for len in 1..=255 {
                test_length(len, config);
            }
        }
    }

    #[test]
    fn test_medium_lengths() {
        for config in TEST_ALL_CONFIGS {
            // Test each length from 256 to 1024, which should fold and include handling remainders
            for len in 256..=1024 {
                test_length(len, config);
            }
        }
    }

    #[test]
    fn test_large_lengths() {
        for config in TEST_ALL_CONFIGS {
            // Test 1 MiB just before, at, and just after the folding boundaries
            for len in 1048575..1048577 {
                test_length(len, config);
            }
        }
    }

    fn test_length(length: usize, config: &AnyCrcTestConfig) {
        let mut data = vec![0u8; length];
        rng().fill(&mut data[..]);

        // Calculate expected CRC using the reference implementation
        let expected = config.checksum_with_reference(&data);

        let result = checksum(config.get_algorithm(), &data);

        assert_eq!(
            result,
            expected,
            "Failed for algorithm: {:?}, length: {}, expected: {:#x}, got: {:#x}",
            config.get_algorithm(),
            length,
            expected,
            result
        );
    }

    #[test]
    fn test_combine() {
        for config in TEST_ALL_CONFIGS {
            let algorithm = config.get_algorithm();
            let check = config.get_check();

            // checksums
            let checksum1 = checksum(algorithm, "1234".as_ref());
            let checksum2 = checksum(algorithm, "56789".as_ref());

            // checksum_combine()
            assert_eq!(checksum_combine(algorithm, checksum1, checksum2, 5), check,);

            // Digest
            let mut digest1 = Digest::new(algorithm);
            digest1.update("1234".as_ref());

            let mut digest2 = Digest::new(algorithm);
            digest2.update("56789".as_ref());

            digest1.combine(&digest2);

            assert_eq!(digest1.finalize(), check)
        }
    }

    #[test]
    fn test_checksum_file() {
        // Create a test file with repeating zeros
        let test_file_path = "test/test_crc32_hash_file.bin";
        let data = vec![0u8; 1024 * 1024]; // 1 MiB of zeros
        if let Err(e) = std::fs::write(test_file_path, &data) {
            eprintln!("Skipping test due to write error: {}", e);
            return;
        }

        for config in TEST_ALL_CONFIGS {
            let result = checksum_file(config.get_algorithm(), test_file_path, None).unwrap();
            assert_eq!(result, config.checksum_with_reference(&data));
        }

        std::fs::remove_file(test_file_path).unwrap();
    }

    #[test]
    fn test_writer() {
        // Create a test file with repeating zeros
        let test_file_path = "test/test_crc32_writer_file.bin";
        let data = vec![0u8; 1024 * 1024]; // 1 MiB of zeros
        if let Err(e) = std::fs::write(test_file_path, &data) {
            eprintln!("Skipping test due to write error: {}", e);
            return;
        }

        for config in TEST_ALL_CONFIGS {
            let mut digest = Digest::new(config.get_algorithm());
            let mut file = File::open(test_file_path).unwrap();
            std::io::copy(&mut file, &mut digest).unwrap();
            assert_eq!(digest.finalize(), config.checksum_with_reference(&data));
        }

        std::fs::remove_file(test_file_path).unwrap();
    }
    #[test]
    fn test_digest_reset() {
        for config in TEST_ALL_CONFIGS {
            let mut digest = Digest::new(config.get_algorithm());
            digest.update(b"42");
            digest.reset();
            digest.update(TEST_CHECK_STRING);
            assert_eq!(digest.finalize(), config.get_check());
        }
    }

    #[test]
    fn test_digest_finalize_reset() {
        for config in TEST_ALL_CONFIGS {
            let check = config.get_check();

            let mut digest = Digest::new(config.get_algorithm());
            digest.update(TEST_CHECK_STRING);
            assert_eq!(digest.finalize_reset(), check);

            digest.update(TEST_CHECK_STRING);
            assert_eq!(digest.finalize(), check);
        }
    }

    #[test]
    fn test_digest_finalize_into() {
        for config in TEST_ALL_CONFIGS {
            let mut digest = Digest::new(config.get_algorithm());
            digest.update(TEST_CHECK_STRING);

            match digest.params.width {
                32 => {
                    let mut output = [0u8; 4];
                    digest.finalize_into(&mut output).unwrap();
                    let result = u32::from_be_bytes(output) as u64;
                    assert_eq!(result, config.get_check());
                }
                64 => {
                    let mut output = [0u8; 8];
                    digest.finalize_into(&mut output).unwrap();
                    let result = u64::from_be_bytes(output);
                    assert_eq!(result, config.get_check());
                }
                _ => panic!("Unsupported CRC width"),
            }
        }
    }

    #[test]
    fn test_digest_finalize_into_reset() {
        for config in TEST_ALL_CONFIGS {
            let mut digest = Digest::new(config.get_algorithm());
            digest.update(TEST_CHECK_STRING);

            let mut output: Vec<u8> = match digest.params.width {
                32 => vec![0u8; 4],
                64 => vec![0u8; 8],
                _ => panic!("Unsupported CRC width"),
            };

            digest.finalize_into_reset(&mut output).unwrap();
            let result = match output.len() {
                4 => u32::from_be_bytes(output.try_into().unwrap()) as u64,
                8 => u64::from_be_bytes(output.try_into().unwrap()),
                _ => panic!("Unsupported CRC width"),
            };
            assert_eq!(result, config.get_check());

            digest.update(TEST_CHECK_STRING);
            assert_eq!(digest.finalize(), config.get_check());
        }
    }

    /// Tests whether the FFI header is up-to-date
    #[test]
    fn test_ffi_header() -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            // Skip this test on Windows, since CRLF vs LF is a PITA
            eprintln!("Skipping test on Windows");

            return Ok(());
        }

        const HEADER: &str = "libcrc_fast.h";

        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|error| error.to_string())?;

        let mut expected = Vec::new();
        cbindgen::Builder::new()
            .with_crate(crate_dir)
            .with_include_guard("CRC_FAST_H")
            .with_header("/* crc_fast library C/C++ API - Copyright 2025 Don MacAskill */\n/* This header is auto-generated. Do not edit directly. */\n")
            // exclude internal implementation functions
            .exclude_item("crc32_iscsi_impl")
            .exclude_item("crc32_iso_hdlc_impl")
            .exclude_item("get_iscsi_target")
            .exclude_item("get_iso_hdlc_target")
            .exclude_item("ISO_HDLC_TARGET")
            .exclude_item("ISCSI_TARGET")
            .exclude_item("CrcParams")
            .rename_item("Digest", "CrcFastDigest")
            .with_style(Both)
            // generate C header
            .with_language(C)
            // with C++ compatibility
            .with_cpp_compat(true)
            .generate()
            .map_err(|error| error.to_string())?
            .write(&mut expected);

        // Convert the expected bytes to string for pattern replacement, since cbindgen
        // generates an annoying amount of empty contiguous newlines
        let header_content = String::from_utf8(expected).map_err(|error| error.to_string())?;

        // Replace excessive newlines (3 or more consecutive newlines) with 2 newlines
        let regex = regex::Regex::new(r"\n{3,}").map_err(|error| error.to_string())?;
        let cleaned_content = regex.replace_all(&header_content, "\n\n").to_string();

        // Convert back to bytes
        expected = cleaned_content.into_bytes();

        let actual = read(HEADER).map_err(|error| error.to_string())?;

        if expected != actual {
            write(HEADER, expected).map_err(|error| error.to_string())?;
            return Err(format!(
                "{HEADER} is not up-to-date, commit the generated file and try again"
            ));
        }

        Ok(())
    }
}
