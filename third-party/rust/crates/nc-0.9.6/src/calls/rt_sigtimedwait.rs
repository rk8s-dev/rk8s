/// Synchronously wait for queued signals.
pub unsafe fn rt_sigtimedwait(
    set: &sigset_t,
    info: Option<&mut siginfo_t>,
    ts: &timespec_t,
) -> Result<i32, Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let info_ptr = info.map_or(core::ptr::null_mut::<siginfo_t>() as usize, |info| {
        info as *mut siginfo_t as usize
    });
    let ts_ptr = ts as *const timespec_t as usize;
    let sig_set_size = core::mem::size_of::<sigset_t>();
    syscall4(SYS_RT_SIGTIMEDWAIT, set_ptr, info_ptr, ts_ptr, sig_set_size).map(|ret| ret as i32)
}
