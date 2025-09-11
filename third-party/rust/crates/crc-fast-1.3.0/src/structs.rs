// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

#![allow(dead_code)]

use crate::arch;
use crate::traits::{CrcCalculator, CrcWidth};
use crate::CrcAlgorithm;

#[derive(Clone, Copy, Debug)]
pub struct CrcParams {
    pub algorithm: CrcAlgorithm,
    pub name: &'static str,
    pub width: u8,
    pub poly: u64,
    pub init: u64,
    pub refin: bool,
    pub refout: bool,
    pub xorout: u64,
    pub check: u64,
    pub keys: [u64; 23],
}

/// CRC-32 width implementation
#[derive(Clone, Copy)]
pub struct Width32;
impl CrcWidth for Width32 {
    const WIDTH: u32 = 32;
    type Value = u32;
}

/// CRC-64 width implementation
#[derive(Clone, Copy)]
pub struct Width64;

impl CrcWidth for Width64 {
    const WIDTH: u32 = 64;
    type Value = u64;
}

/// CRC State wrapper to manage the SIMD operations and reflection mode
#[derive(Debug, Clone, Copy)]
pub struct CrcState<T> {
    pub value: T,
    pub reflected: bool,
}

pub(crate) struct Calculator {}

impl CrcCalculator for Calculator {
    #[inline(always)]
    fn calculate(state: u64, data: &[u8], params: CrcParams) -> u64 {
        unsafe { arch::update(state, data, params) }
    }
}
