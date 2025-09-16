/// Set scheduling paramters.
///
/// # Examples
///
/// ```
/// // This call always returns error because default scheduler is SCHED_NORMAL.
/// // We shall call sched_setscheduler() and change to realtime policy
/// // like SCHED_RR or SCHED_FIFO.
/// let sched_param = nc::sched_param_t { sched_priority: 12 };
/// let ret = unsafe { nc::sched_setparam(0, &sched_param) };
/// assert_eq!(ret, Err(nc::EINVAL));
/// ```
pub unsafe fn sched_setparam(pid: pid_t, param: &sched_param_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let param_ptr = param as *const sched_param_t as usize;
    syscall2(SYS_SCHED_SETPARAM, pid, param_ptr).map(drop)
}
