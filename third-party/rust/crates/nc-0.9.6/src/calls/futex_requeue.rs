/// Requeue a waiter from one futex to another.
///
/// - `waiters`: array describing the source and destination futex
/// - `flags`: unused
/// - `nr_wake`: number of futexes to wake
/// - `nr_requeue`: number of futexes to requeue
///
/// Identical to the traditional `FUTEX_CMP_REQUEUE` op, except it is part of the
/// futex2 family of calls.
pub unsafe fn futex_requeue(
    waiters: &mut [futex_waitv_t],
    flags: u32,
    nr_wake: i32,
    nr_requeue: i32,
) -> Result<(), Errno> {
    let waiters_ptr = waiters.as_mut_ptr() as usize;
    let flags = flags as usize;
    let nr_wake = nr_wake as usize;
    let nr_requeue = nr_requeue as usize;
    syscall4(SYS_FUTEX_REQUEUE, waiters_ptr, flags, nr_wake, nr_requeue).map(drop)
}
