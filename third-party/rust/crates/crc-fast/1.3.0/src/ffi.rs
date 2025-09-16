// Copyright 2025 Don MacAskill. Licensed under MIT or Apache-2.0.

//! FFI bindings for the Rust library
//!
//! This module provides a C-compatible interface for the Rust library, allowing
//! C programs to use the library's functionality.

#![cfg(any(target_arch = "aarch64", target_arch = "x86_64", target_arch = "x86"))]

use crate::CrcAlgorithm;
use crate::{get_calculator_target, Digest};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

/// A handle to the Digest object
#[repr(C)]
pub struct CrcFastDigestHandle(*mut Digest);

/// The supported CRC algorithms
#[repr(C)]
pub enum CrcFastAlgorithm {
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

// Convert from FFI enum to internal enum
impl From<CrcFastAlgorithm> for CrcAlgorithm {
    fn from(value: CrcFastAlgorithm) -> Self {
        match value {
            CrcFastAlgorithm::Crc32Aixm => CrcAlgorithm::Crc32Aixm,
            CrcFastAlgorithm::Crc32Autosar => CrcAlgorithm::Crc32Autosar,
            CrcFastAlgorithm::Crc32Base91D => CrcAlgorithm::Crc32Base91D,
            CrcFastAlgorithm::Crc32Bzip2 => CrcAlgorithm::Crc32Bzip2,
            CrcFastAlgorithm::Crc32CdRomEdc => CrcAlgorithm::Crc32CdRomEdc,
            CrcFastAlgorithm::Crc32Cksum => CrcAlgorithm::Crc32Cksum,
            CrcFastAlgorithm::Crc32Iscsi => CrcAlgorithm::Crc32Iscsi,
            CrcFastAlgorithm::Crc32IsoHdlc => CrcAlgorithm::Crc32IsoHdlc,
            CrcFastAlgorithm::Crc32Jamcrc => CrcAlgorithm::Crc32Jamcrc,
            CrcFastAlgorithm::Crc32Mef => CrcAlgorithm::Crc32Mef,
            CrcFastAlgorithm::Crc32Mpeg2 => CrcAlgorithm::Crc32Mpeg2,
            CrcFastAlgorithm::Crc32Xfer => CrcAlgorithm::Crc32Xfer,
            CrcFastAlgorithm::Crc64Ecma182 => CrcAlgorithm::Crc64Ecma182,
            CrcFastAlgorithm::Crc64GoIso => CrcAlgorithm::Crc64GoIso,
            CrcFastAlgorithm::Crc64Ms => CrcAlgorithm::Crc64Ms,
            CrcFastAlgorithm::Crc64Nvme => CrcAlgorithm::Crc64Nvme,
            CrcFastAlgorithm::Crc64Redis => CrcAlgorithm::Crc64Redis,
            CrcFastAlgorithm::Crc64We => CrcAlgorithm::Crc64We,
            CrcFastAlgorithm::Crc64Xz => CrcAlgorithm::Crc64Xz,
        }
    }
}

/// Creates a new Digest to compute CRC checksums using algorithm
#[no_mangle]
pub extern "C" fn crc_fast_digest_new(algorithm: CrcFastAlgorithm) -> *mut CrcFastDigestHandle {
    let digest = Box::new(Digest::new(algorithm.into()));
    let handle = Box::new(CrcFastDigestHandle(Box::into_raw(digest)));
    Box::into_raw(handle)
}

/// Updates the Digest with data
#[no_mangle]
pub extern "C" fn crc_fast_digest_update(
    handle: *mut CrcFastDigestHandle,
    data: *const c_char,
    len: usize,
) {
    if handle.is_null() || data.is_null() {
        return;
    }

    unsafe {
        let digest = &mut *(*handle).0;

        #[allow(clippy::unnecessary_cast)]
        let bytes = slice::from_raw_parts(data as *const u8, len);
        digest.update(bytes);
    }
}

/// Calculates the CRC checksum for data that's been written to the Digest
#[no_mangle]
pub extern "C" fn crc_fast_digest_finalize(handle: *mut CrcFastDigestHandle) -> u64 {
    if handle.is_null() {
        return 0;
    }

    unsafe {
        let digest = &*(*handle).0;
        digest.finalize()
    }
}

/// Free the Digest resources without finalizing
#[no_mangle]
pub extern "C" fn crc_fast_digest_free(handle: *mut CrcFastDigestHandle) {
    if handle.is_null() {
        return;
    }

    unsafe {
        let handle = Box::from_raw(handle);
        let _ = Box::from_raw(handle.0); // This drops the digest
    }
}

/// Reset the Digest state
#[no_mangle]
pub extern "C" fn crc_fast_digest_reset(handle: *mut CrcFastDigestHandle) {
    if handle.is_null() {
        return;
    }

    unsafe {
        let digest = &mut *(*handle).0;

        digest.reset();
    }
}

/// Finalize and reset the Digest in one operation
#[no_mangle]
pub extern "C" fn crc_fast_digest_finalize_reset(handle: *mut CrcFastDigestHandle) -> u64 {
    if handle.is_null() {
        return 0;
    }

    unsafe {
        let digest = &mut *(*handle).0;

        digest.finalize_reset()
    }
}

/// Combine two Digest checksums
#[no_mangle]
pub extern "C" fn crc_fast_digest_combine(
    handle1: *mut CrcFastDigestHandle,
    handle2: *mut CrcFastDigestHandle,
) {
    if handle1.is_null() || handle2.is_null() {
        return;
    }

    unsafe {
        let digest1 = &mut *(*handle1).0;
        let digest2 = &*(*handle2).0;
        digest1.combine(digest2);
    }
}

/// Gets the amount of data processed by the Digest so far
#[no_mangle]
pub extern "C" fn crc_fast_digest_get_amount(handle: *mut CrcFastDigestHandle) -> u64 {
    if handle.is_null() {
        return 0;
    }

    unsafe {
        let digest = &*(*handle).0;
        digest.get_amount()
    }
}

/// Helper method to calculate a CRC checksum directly for a string using algorithm
#[no_mangle]
pub extern "C" fn crc_fast_checksum(
    algorithm: CrcFastAlgorithm,
    data: *const c_char,
    len: usize,
) -> u64 {
    if data.is_null() {
        return 0;
    }
    unsafe {
        #[allow(clippy::unnecessary_cast)]
        let bytes = slice::from_raw_parts(data as *const u8, len);
        crate::checksum(algorithm.into(), bytes)
    }
}

/// Helper method to just calculate a CRC checksum directly for a file using algorithm
#[no_mangle]
pub extern "C" fn crc_fast_checksum_file(
    algorithm: CrcFastAlgorithm,
    path_ptr: *const u8,
    path_len: usize,
) -> u64 {
    if path_ptr.is_null() {
        return 0;
    }

    unsafe {
        crate::checksum_file(
            algorithm.into(),
            &convert_to_string(path_ptr, path_len),
            None,
        )
        .unwrap()
    }
}

/// Combine two CRC checksums using algorithm
#[no_mangle]
pub extern "C" fn crc_fast_checksum_combine(
    algorithm: CrcFastAlgorithm,
    checksum1: u64,
    checksum2: u64,
    checksum2_len: u64,
) -> u64 {
    crate::checksum_combine(algorithm.into(), checksum1, checksum2, checksum2_len)
}

/// Gets the target build properties (CPU architecture and fine-tuning parameters) for this algorithm
#[no_mangle]
pub extern "C" fn crc_fast_get_calculator_target(algorithm: CrcFastAlgorithm) -> *const c_char {
    let target = get_calculator_target(algorithm.into());

    std::ffi::CString::new(target).unwrap().into_raw()
}

/// Gets the version of this library
#[no_mangle]
pub extern "C" fn crc_fast_get_version() -> *const libc::c_char {
    const VERSION: &CStr =
        match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
            Ok(version) => version,
            Err(_) => panic!("package version contains null bytes??"),
        };

    VERSION.as_ptr()
}

unsafe fn convert_to_string(data: *const u8, len: usize) -> String {
    if data.is_null() {
        return String::new();
    }

    // Safely construct string slice from raw parts
    match std::str::from_utf8(slice::from_raw_parts(data, len)) {
        Ok(s) => s.to_string(),
        Err(_) => panic!("Invalid UTF-8 string"),
    }
}
