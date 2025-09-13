/// Examine pending signals.
pub unsafe fn rt_sigpending(set: &mut sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *mut sigset_t as usize;
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall2(SYS_RT_SIGPENDING, set_ptr, sig_set_size).map(drop)
}
