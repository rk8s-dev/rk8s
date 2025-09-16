// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! This module calculates the keys needed for CRC calculations using PCLMULQDQ.

#![allow(dead_code)]

use std::ops::{BitAnd, BitOr, Shl, Shr};

const CRC32_EXPONENTS: [u64; 23] = [
    0, // unused, just aligns indexes with the literature
    32 * 3,
    32 * 5,
    32 * 31,
    32 * 33,
    32 * 3,
    32 * 2,
    0, // mu, generate separately
    0, // poly, generate separately
    32 * 27,
    32 * 29,
    32 * 23,
    32 * 25,
    32 * 19,
    32 * 21,
    32 * 15,
    32 * 17,
    32 * 11,
    32 * 13,
    32 * 7,
    32 * 9,
    32 * 63, // for 256 byte distances (2048 - 32)
    32 * 65, // for 256 byte distances (2048 + 32)
];

const CRC64_EXPONENTS: [u64; 23] = [
    0, // unused, just aligns indexes with the literature
    64 * 2,
    64 * 3,
    64 * 16,
    64 * 17,
    64 * 2,
    64,
    0, // mu, generate separately
    0, // poly, generate separately
    64 * 14,
    64 * 15,
    64 * 12,
    64 * 13,
    64 * 10,
    64 * 11,
    64 * 8,
    64 * 9,
    64 * 6,
    64 * 7,
    64 * 4,
    64 * 5,
    64 * 32, // for 256 byte distances (2048)
    64 * 33, // for 256 byte distances (2048 + 64)
];

/// Generates the 20 keys needed to calculate CRCs for a given polynomial using PCLMULQDQ when
/// folding by 8.
pub fn keys(width: u8, poly: u64, reflected: bool) -> [u64; 23] {
    let mut keys: [u64; 23] = [0; 23];

    let exponents = if 32 == width {
        CRC32_EXPONENTS
    } else if 64 == width {
        CRC64_EXPONENTS
    } else {
        panic!("Unsupported width: {}", width);
    };

    let poly = if 32 == width {
        poly | (1u64 << 32)
    } else {
        poly
    };

    for i in 1..23 {
        keys[i] = key(width, poly, reflected, exponents[i]);
    }

    keys[7] = mu(width, poly, reflected);
    keys[8] = polynomial(width, poly, reflected);

    keys
}

fn key(width: u8, poly: u64, reflected: bool, exponent: u64) -> u64 {
    if width == 32 {
        crc32_key(exponent, reflected, poly)
    } else if width == 64 {
        crc64_key(exponent, reflected, poly)
    } else {
        panic!("Unsupported width: {}", width);
    }
}

fn crc32_key(exponent: u64, reflected: bool, polynomial: u64) -> u64 {
    if exponent < 32 {
        return 0;
    }

    let mut n: u64 = 0x080000000;
    let e = exponent - 31;

    for _ in 0..e {
        n <<= 1;
        if (n & 0x100000000) != 0 {
            n ^= polynomial;
        }
    }

    if reflected {
        bit_reverse(n) >> 31
    } else {
        n << 32
    }
}

fn crc64_key(exponent: u64, reflected: bool, polynomial: u64) -> u64 {
    if exponent <= 64 {
        return 0;
    }

    let mut n: u64 = 0x8000000000000000;
    let e = if reflected {
        exponent - 64
    } else {
        exponent - 63
    };

    for _ in 0..e {
        n = (n << 1) ^ ((0_u64.wrapping_sub(n >> 63)) & polynomial);
    }

    if reflected {
        bit_reverse(n)
    } else {
        n
    }
}

fn polynomial(width: u8, polynomial: u64, reflected: bool) -> u64 {
    if width == 32 {
        crc32_polynomial(polynomial, reflected)
    } else if width == 64 {
        crc64_polynomial(polynomial, reflected)
    } else {
        panic!("Unsupported width: {}", width);
    }
}

fn crc32_polynomial(polynomial: u64, reflected: bool) -> u64 {
    if !reflected {
        return polynomial | (1u64 << 32);
    };

    // For 32-bit polynomials, operate on full 33 bits including leading 1
    let reversed = bit_reverse((polynomial & 0xFFFFFFFF) as u32);
    // Need to set bit 32 (33rd bit) to get the 1 in the right position after reflection
    ((reversed as u64) << 1) | 1
}

fn crc64_polynomial(polynomial: u64, reflected: bool) -> u64 {
    if !reflected {
        return polynomial | (1u64 << 32);
    };

    // For 64-bit polynomials, operate on all 64 bits
    (bit_reverse(polynomial) << 1) | 1
}

fn mu(width: u8, polynomial: u64, reflected: bool) -> u64 {
    if width == 32 {
        crc32_mu(polynomial, reflected)
    } else if width == 64 {
        crc64_mu(polynomial, reflected)
    } else {
        panic!("Unsupported width: {}", width);
    }
}

fn crc32_mu(polynomial: u64, reflected: bool) -> u64 {
    let mut n: u64 = 0x100000000;
    let mut q: u64 = 0;

    for _ in 0..33 {
        q <<= 1;
        if n & 0x100000000 != 0 {
            q |= 1;
            n ^= polynomial;
        }
        n <<= 1;
    }

    if reflected {
        bit_reverse(q) >> 31
    } else {
        q
    }
}

fn crc64_mu(polynomial: u64, reflected: bool) -> u64 {
    let mut n_hi: u64 = 0x0000000000000001;
    let mut n_lo: u64 = 0x0000000000000000;
    let mut q: u64 = 0;

    let max = if reflected { 64 } else { 65 };

    for _ in 0..max {
        q <<= 1;
        if n_hi != 0 {
            q |= 1;
            n_lo ^= polynomial;
        }
        n_hi = n_lo >> 63;
        n_lo <<= 1;
    }

    if reflected {
        bit_reverse(q)
    } else {
        q
    }
}

fn bit_reverse<T>(mut value: T) -> T
where
    T: Copy
        + Default
        + PartialEq
        + BitAnd<Output = T>
        + BitOr<Output = T>
        + Shl<usize, Output = T>
        + Shr<usize, Output = T>
        + From<u8>,
{
    let one = T::from(1u8);
    let mut result = T::default(); // Zero value

    // Get the bit size of type T
    let bit_size = std::mem::size_of::<T>() * 8;

    for _ in 0..bit_size {
        // Shift result left by 1
        result = result << 1;

        // OR with least significant bit of value
        result = result | (value & one);

        // Shift value right by 1
        value = value >> 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::consts::TEST_ALL_CONFIGS;

    #[test]
    fn test_all() {
        for config in TEST_ALL_CONFIGS {
            let keys = keys(config.get_width(), config.get_poly(), config.get_refin());
            let expected = config.get_keys();

            for (i, key) in keys.iter().enumerate() {
                assert_eq!(
                    *key,
                    expected[i],
                    "Mismatch in keys for {} at index {}: expected 0x{:016x}, got 0x{:016x}",
                    config.get_name(),
                    i,
                    expected[i],
                    *key
                );
            }
        }
    }
}
