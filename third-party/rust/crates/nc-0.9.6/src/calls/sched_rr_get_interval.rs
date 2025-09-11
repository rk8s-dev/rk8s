/// Get the `SCHED_RR` interval for the named process.
///
/// # Examples
///
/// ```
/// let mut ts = nc::timespec_t::default();
/// let ret = unsafe { nc::sched_rr_get_interval(0, &mut ts) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn sched_rr_get_interval(pid: pid_t, interval: &mut timespec_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let interval_ptr = interval as *mut timespec_t as usize;
    syscall2(SYS_SCHED_RR_GET_INTERVAL, pid, interval_ptr).map(drop)
}
