/// Get scheduling paramters.
///
/// # Examples
///
/// ```
/// let mut param = nc::sched_param_t::default();
/// let ret = unsafe { nc::sched_getparam(0, &mut param) };
/// assert!(ret.is_ok());
/// assert_eq!(param.sched_priority, 0);
/// ```
pub unsafe fn sched_getparam(pid: pid_t, param: &mut sched_param_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let param_ptr = param as *mut sched_param_t as usize;
    syscall2(SYS_SCHED_GETPARAM, pid, param_ptr).map(drop)
}
