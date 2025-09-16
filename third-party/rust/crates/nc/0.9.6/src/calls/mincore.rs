/// `mincore()` returns the memory residency status of the pages in the
/// current process's address space specified by `[addr, addr + len)`.
///
/// The status is returned in a vector of bytes.  The least significant
/// bit of each byte is 1 if the referenced page is in memory, otherwise
/// it is zero.
///
/// Because the status of a page can change after `mincore()` checks it
/// but before it returns to the application, the returned vector may
/// contain stale information.  Only locked pages are guaranteed to
/// remain in memory.
///
/// return values:
///  zero    - success
///  -EFAULT - vec points to an illegal address
///  -EINVAL - addr is not a multiple of `PAGE_SIZE`
///  -ENOMEM - Addresses in the range `[addr, addr + len]` are
/// invalid for the address space of this process, or specify one or
/// more pages which are not currently mapped
///  -EAGAIN - A kernel resource was temporarily unavailable.
pub unsafe fn mincore(
    start: *const core::ffi::c_void,
    len: size_t,
    vec: &mut [u8],
) -> Result<(), Errno> {
    let start = start as usize;
    let vec_ptr = vec.as_ptr() as usize;
    syscall3(SYS_MINCORE, start, len, vec_ptr).map(drop)
}
