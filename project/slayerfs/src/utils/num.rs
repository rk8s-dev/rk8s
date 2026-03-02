pub(crate) trait NumCastExt {
    fn as_u32(&self) -> u32;
    fn as_usize(&self) -> usize;
    fn as_i64(&self) -> i64;
}

impl NumCastExt for u64 {
    #[inline]
    fn as_u32(&self) -> u32 {
        debug_assert!(*self <= u32::MAX as u64);
        *self as u32
    }

    #[inline]
    fn as_usize(&self) -> usize {
        debug_assert!(*self <= usize::MAX as u64);
        *self as usize
    }

    #[inline]
    fn as_i64(&self) -> i64 {
        debug_assert!(*self <= i64::MAX as u64);
        *self as i64
    }
}
