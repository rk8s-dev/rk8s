/// Correct the time to allow synchronization of the system clock.
pub unsafe fn adjtime(delta: &timeval_t, old_delta: &mut timeval_t) -> Result<(), Errno> {
    let delta_ptr = delta as *const timeval_t as usize;
    let old_delta_ptr = old_delta as *mut timeval_t as usize;
    syscall2(SYS_ADJTIME, delta_ptr, old_delta_ptr).map(drop)
}
