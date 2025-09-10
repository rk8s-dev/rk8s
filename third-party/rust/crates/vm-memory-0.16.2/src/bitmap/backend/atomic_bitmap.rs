// Copyright 2021 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Bitmap backend implementation based on atomic integers.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bitmap::{Bitmap, RefSlice, WithBitmapSlice};

#[cfg(feature = "backend-mmap")]
use crate::mmap::NewBitmap;

/// `AtomicBitmap` implements a simple bit map on the page level with test and set operations.
/// It is page-size aware, so it converts addresses to page numbers before setting or clearing
/// the bits.
#[derive(Debug)]
pub struct AtomicBitmap {
    map: Vec<AtomicU64>,
    size: usize,
    byte_size: usize,
    page_size: NonZeroUsize,
}

#[allow(clippy::len_without_is_empty)]
impl AtomicBitmap {
    /// Create a new bitmap of `byte_size`, with one bit per page. This is effectively
    /// rounded up, and we get a new vector of the next multiple of 64 bigger than `bit_size`.
    pub fn new(byte_size: usize, page_size: NonZeroUsize) -> Self {
        let num_pages = byte_size.div_ceil(page_size.get());
        let map_size = num_pages.div_ceil(u64::BITS as usize);
        let map: Vec<AtomicU64> = (0..map_size).map(|_| AtomicU64::new(0)).collect();

        AtomicBitmap {
            map,
            size: num_pages,
            byte_size,
            page_size,
        }
    }

    /// Enlarge this bitmap with enough bits to track `additional_size` additional bytes at page granularity.
    /// New bits are initialized to zero.
    pub fn enlarge(&mut self, additional_size: usize) {
        self.byte_size += additional_size;
        self.size = self.byte_size.div_ceil(self.page_size.get());
        let map_size = self.size.div_ceil(u64::BITS as usize);
        self.map.resize_with(map_size, Default::default);
    }

    /// Is bit `n` set? Bits outside the range of the bitmap are always unset.
    pub fn is_bit_set(&self, index: usize) -> bool {
        if index < self.size {
            (self.map[index >> 6].load(Ordering::Acquire) & (1 << (index & 63))) != 0
        } else {
            // Out-of-range bits are always unset.
            false
        }
    }

    /// Is the bit corresponding to address `addr` set?
    pub fn is_addr_set(&self, addr: usize) -> bool {
        self.is_bit_set(addr / self.page_size)
    }

    /// Set a range of `len` bytes starting at `start_addr`. The first bit set in the bitmap
    /// is for the page corresponding to `start_addr`, and the last bit that we set corresponds
    /// to address `start_addr + len - 1`.
    pub fn set_addr_range(&self, start_addr: usize, len: usize) {
        self.set_reset_addr_range(start_addr, len, true);
    }

    // Set/Reset a range of `len` bytes starting at `start_addr`
    // reset parameter determines whether bit will be set/reset
    // if set is true then the range of bits will be set to one,
    // otherwise zero
    fn set_reset_addr_range(&self, start_addr: usize, len: usize, set: bool) {
        // Return early in the unlikely event that `len == 0` so the `len - 1` computation
        // below does not underflow.
        if len == 0 {
            return;
        }

        let first_bit = start_addr / self.page_size;
        // Handle input ranges where `start_addr + len - 1` would otherwise overflow an `usize`
        // by ignoring pages at invalid addresses.
        let last_bit = start_addr.saturating_add(len - 1) / self.page_size;
        for n in first_bit..=last_bit {
            if n >= self.size {
                // Attempts to set bits beyond the end of the bitmap are simply ignored.
                break;
            }
            if set {
                self.map[n >> 6].fetch_or(1 << (n & 63), Ordering::SeqCst);
            } else {
                self.map[n >> 6].fetch_and(!(1 << (n & 63)), Ordering::SeqCst);
            }
        }
    }

    /// Reset a range of `len` bytes starting at `start_addr`. The first bit set in the bitmap
    /// is for the page corresponding to `start_addr`, and the last bit that we set corresponds
    /// to address `start_addr + len - 1`.
    pub fn reset_addr_range(&self, start_addr: usize, len: usize) {
        self.set_reset_addr_range(start_addr, len, false);
    }

    /// Set bit to corresponding index
    pub fn set_bit(&self, index: usize) {
        if index >= self.size {
            // Attempts to set bits beyond the end of the bitmap are simply ignored.
            return;
        }
        self.map[index >> 6].fetch_or(1 << (index & 63), Ordering::SeqCst);
    }

    /// Reset bit to corresponding index
    pub fn reset_bit(&self, index: usize) {
        if index >= self.size {
            // Attempts to reset bits beyond the end of the bitmap are simply ignored.
            return;
        }
        self.map[index >> 6].fetch_and(!(1 << (index & 63)), Ordering::SeqCst);
    }

    /// Get the length of the bitmap in bits (i.e. in how many pages it can represent).
    pub fn len(&self) -> usize {
        self.size
    }

    /// Get the size in bytes i.e how many bytes the bitmap can represent, one bit per page.
    pub fn byte_size(&self) -> usize {
        self.byte_size
    }

    /// Atomically get and reset the dirty page bitmap.
    pub fn get_and_reset(&self) -> Vec<u64> {
        self.map
            .iter()
            .map(|u| u.fetch_and(0, Ordering::SeqCst))
            .collect()
    }

    /// Reset all bitmap bits to 0.
    pub fn reset(&self) {
        for it in self.map.iter() {
            it.store(0, Ordering::Release);
        }
    }
}

impl Clone for AtomicBitmap {
    fn clone(&self) -> Self {
        let map = self
            .map
            .iter()
            .map(|i| i.load(Ordering::Acquire))
            .map(AtomicU64::new)
            .collect();
        AtomicBitmap {
            map,
            size: self.size,
            byte_size: self.byte_size,
            page_size: self.page_size,
        }
    }
}

impl<'a> WithBitmapSlice<'a> for AtomicBitmap {
    type S = RefSlice<'a, Self>;
}

impl Bitmap for AtomicBitmap {
    fn mark_dirty(&self, offset: usize, len: usize) {
        self.set_addr_range(offset, len)
    }

    fn dirty_at(&self, offset: usize) -> bool {
        self.is_addr_set(offset)
    }

    fn slice_at(&self, offset: usize) -> <Self as WithBitmapSlice>::S {
        RefSlice::new(self, offset)
    }
}

impl Default for AtomicBitmap {
    fn default() -> Self {
        // SAFETY: Safe as `0x1000` is non-zero.
        AtomicBitmap::new(0, unsafe { NonZeroUsize::new_unchecked(0x1000) })
    }
}

#[cfg(feature = "backend-mmap")]
impl NewBitmap for AtomicBitmap {
    fn with_len(len: usize) -> Self {
        #[cfg(unix)]
        // SAFETY: There's no unsafe potential in calling this function.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) };

        #[cfg(windows)]
        let page_size = {
            use winapi::um::sysinfoapi::{GetSystemInfo, LPSYSTEM_INFO, SYSTEM_INFO};
            let mut sysinfo = MaybeUninit::zeroed();
            // SAFETY: It's safe to call `GetSystemInfo` as `sysinfo` is rightly sized
            // allocated memory.
            unsafe { GetSystemInfo(sysinfo.as_mut_ptr()) };
            // SAFETY: It's safe to call `assume_init` as `GetSystemInfo` initializes `sysinfo`.
            unsafe { sysinfo.assume_init().dwPageSize }
        };

        // The `unwrap` is safe to use because the above call should always succeed on the
        // supported platforms, and the size of a page will always fit within a `usize`.
        AtomicBitmap::new(
            len,
            NonZeroUsize::try_from(usize::try_from(page_size).unwrap()).unwrap(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bitmap::tests::test_bitmap;

    #[allow(clippy::undocumented_unsafe_blocks)]
    const DEFAULT_PAGE_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(128) };

    #[test]
    fn test_bitmap_basic() {
        // Test that bitmap size is properly rounded up.
        let a = AtomicBitmap::new(1025, DEFAULT_PAGE_SIZE);
        assert_eq!(a.len(), 9);

        let b = AtomicBitmap::new(1024, DEFAULT_PAGE_SIZE);
        assert_eq!(b.len(), 8);
        b.set_addr_range(128, 129);
        assert!(!b.is_addr_set(0));
        assert!(b.is_addr_set(128));
        assert!(b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        #[allow(clippy::redundant_clone)]
        let copy_b = b.clone();
        assert!(copy_b.is_addr_set(256));
        assert!(!copy_b.is_addr_set(384));

        b.reset();
        assert!(!b.is_addr_set(128));
        assert!(!b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        b.set_addr_range(128, 129);
        let v = b.get_and_reset();

        assert!(!b.is_addr_set(128));
        assert!(!b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        assert_eq!(v.len(), 1);
        assert_eq!(v[0], 0b110);
    }

    #[test]
    fn test_bitmap_reset() {
        let b = AtomicBitmap::new(1024, DEFAULT_PAGE_SIZE);
        assert_eq!(b.len(), 8);
        b.set_addr_range(128, 129);
        assert!(!b.is_addr_set(0));
        assert!(b.is_addr_set(128));
        assert!(b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        b.reset_addr_range(128, 129);
        assert!(!b.is_addr_set(0));
        assert!(!b.is_addr_set(128));
        assert!(!b.is_addr_set(256));
        assert!(!b.is_addr_set(384));
    }

    #[test]
    fn test_bitmap_out_of_range() {
        let b = AtomicBitmap::new(1024, NonZeroUsize::MIN);
        // Set a partial range that goes beyond the end of the bitmap
        b.set_addr_range(768, 512);
        assert!(b.is_addr_set(768));
        // The bitmap is never set beyond its end.
        assert!(!b.is_addr_set(1024));
        assert!(!b.is_addr_set(1152));
    }

    #[test]
    fn test_bitmap_impl() {
        let b = AtomicBitmap::new(0x800, DEFAULT_PAGE_SIZE);
        test_bitmap(&b);
    }

    #[test]
    fn test_bitmap_enlarge() {
        let mut b = AtomicBitmap::new(8 * 1024, DEFAULT_PAGE_SIZE);
        assert_eq!(b.len(), 64);
        b.set_addr_range(128, 129);
        assert!(!b.is_addr_set(0));
        assert!(b.is_addr_set(128));
        assert!(b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        b.reset_addr_range(128, 129);
        assert!(!b.is_addr_set(0));
        assert!(!b.is_addr_set(128));
        assert!(!b.is_addr_set(256));
        assert!(!b.is_addr_set(384));
        b.set_addr_range(128, 129);
        b.enlarge(8 * 1024);
        for i in 65..128 {
            assert!(!b.is_bit_set(i));
        }
        assert_eq!(b.len(), 128);
        assert!(!b.is_addr_set(0));
        assert!(b.is_addr_set(128));
        assert!(b.is_addr_set(256));
        assert!(!b.is_addr_set(384));

        b.set_bit(55);
        assert!(b.is_bit_set(55));
        for i in 65..128 {
            b.set_bit(i);
        }
        for i in 65..128 {
            assert!(b.is_bit_set(i));
        }
        b.reset_addr_range(0, 16 * 1024);
        for i in 0..128 {
            assert!(!b.is_bit_set(i));
        }
    }
}
