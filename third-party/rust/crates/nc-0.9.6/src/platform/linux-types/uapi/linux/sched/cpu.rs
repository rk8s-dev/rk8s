// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! `cput_set_t` is from libc.

#![allow(clippy::module_name_repetitions)]

use core::mem::size_of;

use crate::{Errno, EINVAL};

pub const CPU_SET_BYTES: usize = 128;
const WORD_BYTES: usize = size_of::<u64>();
const WORD_BITS: usize = WORD_BYTES * 8;

#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct cpu_set_t {
    pub bits: [usize; CPU_SET_BYTES / WORD_BYTES],
}

impl cpu_set_t {
    #[must_use]
    #[inline]
    pub const fn size() -> usize {
        CPU_SET_BYTES / WORD_BYTES
    }

    #[must_use]
    #[inline]
    pub const fn bits() -> usize {
        CPU_SET_BYTES * 8
    }

    /// # Errors
    ///
    /// Returns `EINVAL` if `pos` is larger than 1023.
    pub fn set(&mut self, pos: usize) -> Result<(), Errno> {
        if pos >= Self::bits() {
            return Err(EINVAL);
        }
        let bit_pos = pos / WORD_BITS;
        self.bits[bit_pos] |= 1 << (pos % (WORD_BITS));
        Ok(())
    }

    /// # Errors
    ///
    /// Returns `EINVAL` if `pos` is larger than 1023.
    pub fn clear(&mut self, pos: usize) -> Result<(), Errno> {
        if pos >= Self::bits() {
            return Err(EINVAL);
        }
        let bit_pos = pos / WORD_BITS;
        self.bits[bit_pos] &= !(1 << (pos % WORD_BITS));
        Ok(())
    }

    /// # Errors
    ///
    /// Returns `EINVAL` if `pos` is larger than 1023.
    pub const fn is_set(&self, pos: usize) -> Result<bool, Errno> {
        if pos >= Self::bits() {
            return Err(EINVAL);
        }
        let bit_pos = pos / WORD_BITS;
        let ret = self.bits[bit_pos] & (1 << (pos % WORD_BITS));

        Ok(ret != 0)
    }

    #[must_use]
    #[inline]
    pub const fn as_ptr(&self) -> &[usize] {
        &self.bits
    }

    #[must_use]
    #[inline]
    pub fn as_mut_ptr(&mut self) -> &mut [usize] {
        &mut self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::cpu_set_t;

    #[test]
    fn test_size() {
        assert_eq!(cpu_set_t::size(), 16);
    }

    #[test]
    fn test_bits() {
        assert_eq!(cpu_set_t::bits(), 1024);
    }
}
