/// Wake a number of futexes.
///
/// - `uaddr`: Address of the futex(es) to wake
/// - `mask`: bitmask
/// - `nr`: Number of the futexes to wake
/// - `flags`: `FUTEX2` flags
///
/// Identical to the traditional `FUTEX_WAKE_BITSET` op, except it is part of the
/// futex2 family of calls.
pub unsafe fn futex_wake(
    uaddr: *const core::ffi::c_void,
    mask: usize,
    nr: i32,
    flags: u32,
) -> Result<(), Errno> {
    let uaddr = uaddr as usize;
    let nr = nr as usize;
    let flags = flags as usize;
    syscall4(SYS_FUTEX_WAKE, uaddr, mask, nr, flags).map(drop)
}
