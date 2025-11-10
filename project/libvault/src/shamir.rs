//! A Shamir threshold algorithm implementation which is used to derive the RustyVault master key.
//!
//! This code is originated from Chris MacNaughton, we modified it to be compatible with Vault's
//! Shamir algorithm.
//!
//! The original MIT license is as follows:
//!
//! The MIT License (MIT)
//!
//! Copyright (c) 2016 Chris MacNaughton
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::ops::DerefMut;

use rand::{RngCore, thread_rng};
use zeroize::Zeroizing;

use crate::errors::RvError;

static GF256_EXP: [u8; 256] = [
    0x01, 0xe5, 0x4c, 0xb5, 0xfb, 0x9f, 0xfc, 0x12, 0x03, 0x34, 0xd4, 0xc4, 0x16, 0xba, 0x1f, 0x36,
    0x05, 0x5c, 0x67, 0x57, 0x3a, 0xd5, 0x21, 0x5a, 0x0f, 0xe4, 0xa9, 0xf9, 0x4e, 0x64, 0x63, 0xee,
    0x11, 0x37, 0xe0, 0x10, 0xd2, 0xac, 0xa5, 0x29, 0x33, 0x59, 0x3b, 0x30, 0x6d, 0xef, 0xf4, 0x7b,
    0x55, 0xeb, 0x4d, 0x50, 0xb7, 0x2a, 0x07, 0x8d, 0xff, 0x26, 0xd7, 0xf0, 0xc2, 0x7e, 0x09, 0x8c,
    0x1a, 0x6a, 0x62, 0x0b, 0x5d, 0x82, 0x1b, 0x8f, 0x2e, 0xbe, 0xa6, 0x1d, 0xe7, 0x9d, 0x2d, 0x8a,
    0x72, 0xd9, 0xf1, 0x27, 0x32, 0xbc, 0x77, 0x85, 0x96, 0x70, 0x08, 0x69, 0x56, 0xdf, 0x99, 0x94,
    0xa1, 0x90, 0x18, 0xbb, 0xfa, 0x7a, 0xb0, 0xa7, 0xf8, 0xab, 0x28, 0xd6, 0x15, 0x8e, 0xcb, 0xf2,
    0x13, 0xe6, 0x78, 0x61, 0x3f, 0x89, 0x46, 0x0d, 0x35, 0x31, 0x88, 0xa3, 0x41, 0x80, 0xca, 0x17,
    0x5f, 0x53, 0x83, 0xfe, 0xc3, 0x9b, 0x45, 0x39, 0xe1, 0xf5, 0x9e, 0x19, 0x5e, 0xb6, 0xcf, 0x4b,
    0x38, 0x04, 0xb9, 0x2b, 0xe2, 0xc1, 0x4a, 0xdd, 0x48, 0x0c, 0xd0, 0x7d, 0x3d, 0x58, 0xde, 0x7c,
    0xd8, 0x14, 0x6b, 0x87, 0x47, 0xe8, 0x79, 0x84, 0x73, 0x3c, 0xbd, 0x92, 0xc9, 0x23, 0x8b, 0x97,
    0x95, 0x44, 0xdc, 0xad, 0x40, 0x65, 0x86, 0xa2, 0xa4, 0xcc, 0x7f, 0xec, 0xc0, 0xaf, 0x91, 0xfd,
    0xf7, 0x4f, 0x81, 0x2f, 0x5b, 0xea, 0xa8, 0x1c, 0x02, 0xd1, 0x98, 0x71, 0xed, 0x25, 0xe3, 0x24,
    0x06, 0x68, 0xb3, 0x93, 0x2c, 0x6f, 0x3e, 0x6c, 0x0a, 0xb8, 0xce, 0xae, 0x74, 0xb1, 0x42, 0xb4,
    0x1e, 0xd3, 0x49, 0xe9, 0x9c, 0xc8, 0xc6, 0xc7, 0x22, 0x6e, 0xdb, 0x20, 0xbf, 0x43, 0x51, 0x52,
    0x66, 0xb2, 0x76, 0x60, 0xda, 0xc5, 0xf3, 0xf6, 0xaa, 0xcd, 0x9a, 0xa0, 0x75, 0x54, 0x0e, 0x01,
];

static GF256_LOG: [u8; 256] = [
    0x00, 0xff, 0xc8, 0x08, 0x91, 0x10, 0xd0, 0x36, 0x5a, 0x3e, 0xd8, 0x43, 0x99, 0x77, 0xfe, 0x18,
    0x23, 0x20, 0x07, 0x70, 0xa1, 0x6c, 0x0c, 0x7f, 0x62, 0x8b, 0x40, 0x46, 0xc7, 0x4b, 0xe0, 0x0e,
    0xeb, 0x16, 0xe8, 0xad, 0xcf, 0xcd, 0x39, 0x53, 0x6a, 0x27, 0x35, 0x93, 0xd4, 0x4e, 0x48, 0xc3,
    0x2b, 0x79, 0x54, 0x28, 0x09, 0x78, 0x0f, 0x21, 0x90, 0x87, 0x14, 0x2a, 0xa9, 0x9c, 0xd6, 0x74,
    0xb4, 0x7c, 0xde, 0xed, 0xb1, 0x86, 0x76, 0xa4, 0x98, 0xe2, 0x96, 0x8f, 0x02, 0x32, 0x1c, 0xc1,
    0x33, 0xee, 0xef, 0x81, 0xfd, 0x30, 0x5c, 0x13, 0x9d, 0x29, 0x17, 0xc4, 0x11, 0x44, 0x8c, 0x80,
    0xf3, 0x73, 0x42, 0x1e, 0x1d, 0xb5, 0xf0, 0x12, 0xd1, 0x5b, 0x41, 0xa2, 0xd7, 0x2c, 0xe9, 0xd5,
    0x59, 0xcb, 0x50, 0xa8, 0xdc, 0xfc, 0xf2, 0x56, 0x72, 0xa6, 0x65, 0x2f, 0x9f, 0x9b, 0x3d, 0xba,
    0x7d, 0xc2, 0x45, 0x82, 0xa7, 0x57, 0xb6, 0xa3, 0x7a, 0x75, 0x4f, 0xae, 0x3f, 0x37, 0x6d, 0x47,
    0x61, 0xbe, 0xab, 0xd3, 0x5f, 0xb0, 0x58, 0xaf, 0xca, 0x5e, 0xfa, 0x85, 0xe4, 0x4d, 0x8a, 0x05,
    0xfb, 0x60, 0xb7, 0x7b, 0xb8, 0x26, 0x4a, 0x67, 0xc6, 0x1a, 0xf8, 0x69, 0x25, 0xb3, 0xdb, 0xbd,
    0x66, 0xdd, 0xf1, 0xd2, 0xdf, 0x03, 0x8d, 0x34, 0xd9, 0x92, 0x0d, 0x63, 0x55, 0xaa, 0x49, 0xec,
    0xbc, 0x95, 0x3c, 0x84, 0x0b, 0xf5, 0xe6, 0xe7, 0xe5, 0xac, 0x7e, 0x6e, 0xb9, 0xf9, 0xda, 0x8e,
    0x9a, 0xc9, 0x24, 0xe1, 0x0a, 0x15, 0x6b, 0x3a, 0xa0, 0x51, 0xf4, 0xea, 0xb2, 0x97, 0x9e, 0x5d,
    0x22, 0x88, 0x94, 0xce, 0x19, 0x01, 0x71, 0x4c, 0xa5, 0xe3, 0xc5, 0x31, 0xbb, 0xcc, 0x1f, 0x2d,
    0x3b, 0x52, 0x6f, 0xf6, 0x2e, 0x89, 0xf7, 0xc0, 0x68, 0x1b, 0x64, 0x04, 0x06, 0xbf, 0x83, 0x38,
];

pub const SHAMIR_OVERHEAD: usize = 1;

pub struct ShamirSecret {
    pub coefficients: Vec<Vec<u8>>,
}

impl ShamirSecret {
    pub fn with_secret(secret: &[u8], threshold: u8) -> ShamirSecret {
        let mut coefficients: Vec<Vec<u8>> = vec![];
        let mut rng = thread_rng();
        let mut rand_container = vec![0u8; (threshold - 1) as usize];
        for c in secret {
            rng.fill_bytes(&mut rand_container);
            let mut coef: Vec<u8> = vec![*c];
            for r in rand_container.iter() {
                coef.push(*r);
            }
            coefficients.push(coef);
        }

        ShamirSecret { coefficients }
    }

    pub fn get_share(&self, id: u8) -> Result<Vec<u8>, RvError> {
        if id == 0 {
            return Err(RvError::ErrShamirShareCountInvalid);
        }
        let mut share_bytes: Vec<u8> = vec![];
        let coefficients = self.coefficients.clone();
        for coefficient in coefficients {
            let b = ShamirSecret::accumulate_share_bytes(id, coefficient)?;
            share_bytes.push(b);
        }

        share_bytes.push(id);
        Ok(share_bytes)
    }

    pub fn is_valid_share(&self, share: &[u8]) -> bool {
        let id = share[share.len() - 1];
        match self.get_share(id) {
            Ok(s) => s == share,
            _ => false,
        }
    }

    pub fn recover_secret(shares: Vec<Vec<u8>>) -> Option<Vec<u8>> {
        if shares.len() < 2 {
            println!("Less than two parts cannot be used to reconstruct the secret");
            return None;
        }
        let mut xs: Vec<u8> = vec![];

        for share in shares.iter() {
            if share.is_empty() {
                return None;
            }

            let last = share.last().unwrap();

            if xs.contains(last) {
                println!("Multiple shares with the same first byte");
                return None;
            }

            if share.len() != shares[0].len() {
                println!("Shares have different lengths");
                return None;
            }

            xs.push(last.to_owned());
        }
        let mut mysecretdata: Vec<u8> = vec![];
        let rounds = shares[0].len() - 1;

        for byte_to_use in 0..rounds {
            let mut fxs: Vec<u8> = vec![];
            for share in shares.clone() {
                fxs.push(share[0..share.len()][byte_to_use]);
            }

            match ShamirSecret::full_lagrange(&xs, &fxs) {
                None => return None,
                Some(resulting_poly) => {
                    mysecretdata.push(resulting_poly[0]);
                }
            }
        }

        Some(mysecretdata)
    }

    pub fn split(
        secret: &[u8],
        part: u8,
        threshold: u8,
    ) -> Result<Zeroizing<Vec<Vec<u8>>>, RvError> {
        if part < threshold || threshold < 2 || part == 255 {
            return Err(RvError::ErrShamirShareCountInvalid);
        }

        let secret_data = ShamirSecret::with_secret(secret, threshold);
        let mut out: Zeroizing<Vec<Vec<u8>>> = Zeroizing::new(vec![]);
        for i in 1..(part + 1) {
            let shared = secret_data.get_share(i)?;
            out.deref_mut().push(shared);
        }
        Ok(out)
    }

    pub fn combine(shares: Vec<Vec<u8>>) -> Option<Vec<u8>> {
        ShamirSecret::recover_secret(shares)
    }

    fn accumulate_share_bytes(id: u8, coefficient_bytes: Vec<u8>) -> Result<u8, RvError> {
        if id == 0 {
            return Err(RvError::ErrShamirShareCountInvalid);
        }
        let mut accumulator: u8 = 0;

        let mut x_i: u8 = 1;

        for c in coefficient_bytes {
            accumulator = ShamirSecret::gf256_add(accumulator, ShamirSecret::gf256_mul(c, x_i));
            x_i = ShamirSecret::gf256_mul(x_i, id);
        }

        Ok(accumulator)
    }

    fn full_lagrange(xs: &[u8], fxs: &[u8]) -> Option<Vec<u8>> {
        let mut returned_coefficients: Vec<u8> = vec![];
        let len = fxs.len();
        for i in 0..len {
            let mut this_polynomial: Vec<u8> = vec![1];

            for j in 0..len {
                if i == j {
                    continue;
                }

                let denominator = ShamirSecret::gf256_sub(xs[i], xs[j]);
                let first_term = ShamirSecret::gf256_checked_div(xs[j], denominator);
                let second_term = ShamirSecret::gf256_checked_div(1, denominator);
                match (first_term, second_term) {
                    (Some(a), Some(b)) => {
                        let this_term = vec![a, b];
                        this_polynomial =
                            ShamirSecret::multiply_polynomials(&this_polynomial, &this_term);
                    }
                    (_, _) => return None,
                };
            }
            if fxs.len() + 1 >= i {
                this_polynomial = ShamirSecret::multiply_polynomials(&this_polynomial, &[fxs[i]])
            }
            returned_coefficients =
                ShamirSecret::add_polynomials(&returned_coefficients, &this_polynomial);
        }
        Some(returned_coefficients)
    }

    #[inline]
    fn gf256_add(a: u8, b: u8) -> u8 {
        a ^ b
    }

    #[inline]
    fn gf256_sub(a: u8, b: u8) -> u8 {
        ShamirSecret::gf256_add(a, b)
    }

    #[inline]
    fn gf256_mul(a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            GF256_EXP[((u16::from(GF256_LOG[a as usize]) + u16::from(GF256_LOG[b as usize])) % 255)
                as usize]
        }
    }

    #[inline]
    fn gf256_checked_div(a: u8, b: u8) -> Option<u8> {
        if a == 0 {
            Some(0)
        } else if b == 0 {
            None
        } else {
            let a_log = i16::from(GF256_LOG[a as usize]);
            let b_log = i16::from(GF256_LOG[b as usize]);

            let mut diff = a_log - b_log;

            if diff < 0 {
                diff += 255;
            }
            Some(GF256_EXP[(diff % 255) as usize])
        }
    }

    #[inline]
    fn multiply_polynomials(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut resultterms: Vec<u8> = vec![];

        let mut termpadding: Vec<u8> = vec![];

        for bterm in b {
            let mut thisvalue = termpadding.clone();
            for aterm in a {
                thisvalue.push(ShamirSecret::gf256_mul(*aterm, *bterm));
            }
            resultterms = ShamirSecret::add_polynomials(&resultterms, &thisvalue);
            termpadding.push(0);
        }
        resultterms
    }

    #[inline]
    fn add_polynomials(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut a = a.to_owned();
        let mut b = b.to_owned();

        match a.len().cmp(&b.len()) {
            std::cmp::Ordering::Less => {
                let mut t = vec![0; b.len() - a.len()];
                a.append(&mut t);
            }
            std::cmp::Ordering::Greater => {
                let mut t = vec![0; a.len() - b.len()];
                b.append(&mut t);
            }
            std::cmp::Ordering::Equal => {}
        }

        let mut results: Vec<u8> = vec![];
        for i in 0..a.len() {
            results.push(ShamirSecret::gf256_add(a[i], b[i]));
        }
        results
    }
}
