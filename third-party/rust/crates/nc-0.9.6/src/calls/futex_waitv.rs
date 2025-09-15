/// Wait on a list of futexes.
///
/// - `waiters`: List of futexes to wait on
/// - `nr_futexes`: Length of futexv
/// - `flags`: Flag for timeout (monotonic/realtime)
/// - `timeout`: Optional absolute timeout.
/// - `clockid`: Clock to be used for the timeout, realtime or monotonic.
///
/// Given an array of `struct futex_waitv_t`, wait on each uaddr.
/// The thread wakes if a `futex_wake()` is performed at any uaddr.
/// The syscall returns immediately if any waiter has `*uaddr != val`.
///
/// `timeout` is an optional timeout value for the operation.
///
/// Each waiter has individual flags. The `flags` argument for the syscall
/// should be used solely for specifying the timeout as realtime, if needed.
/// Flags for private futexes, sizes, etc. should be used on the individual flags
/// of each waiter.
///
/// Returns the array index of one of the woken futexes. No further information
/// is provided: any number of other futexes may also have been woken by the
/// same event, and if more than one futex was woken, the retrned index may
/// refer to any one of them. (It is not necessaryily the futex with the
/// smallest index, nor the one most recently woken, nor...)
pub unsafe fn futex_waitv(
    waiters: &[futex_waitv_t],
    flags: u32,
    timeout: Option<&timespec_t>,
    clockid: clockid_t,
) -> Result<i32, Errno> {
    let waiters_ptr = waiters.as_ptr() as usize;
    let waiters_len = waiters.len();
    let flags = flags as usize;
    let timeout_ptr = timeout.map_or(core::ptr::null::<timespec_t>() as usize, |timeout| {
        timeout as *const timespec_t as usize
    });
    let clockid = clockid as usize;
    syscall5(
        SYS_FUTEX_WAITV,
        waiters_ptr,
        waiters_len,
        flags,
        timeout_ptr,
        clockid,
    )
    .map(|ret| ret as i32)
}
