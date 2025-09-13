// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! This module provides tests and utilities for the CRC library.

#![cfg(test)]
#![allow(dead_code)]

pub(crate) mod consts;
pub(crate) mod enums;
mod structs;

/// Creates a new aligned data vector from the input slice for testing.
pub(crate) fn create_aligned_data(input: &[u8]) -> Vec<u8> {
    // Size of our target alignment structure
    let align_size = std::mem::size_of::<[[u64; 4]; 2]>(); // 64 bytes

    // Create a vector with padding to ensure we can find a properly aligned position
    let mut padded = Vec::with_capacity(input.len() + align_size);

    // Fill with zeros initially to reach needed capacity
    padded.resize(input.len() + align_size, 0);

    // Find the first address that satisfies our alignment
    let start_addr = padded.as_ptr() as usize;
    let align_offset = (align_size - (start_addr % align_size)) % align_size;

    // Copy the input into the aligned position
    let aligned_start = &mut padded[align_offset..];
    aligned_start[..input.len()].copy_from_slice(input);

    // Return the exact slice we need
    aligned_start[..input.len()].to_vec()
}
