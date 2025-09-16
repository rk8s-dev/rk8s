/// Get scheduling parameter.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::sched_getscheduler(0) };
/// assert_eq!(ret, Ok(nc::SCHED_NORMAL));
/// ```
pub unsafe fn sched_getscheduler(pid: pid_t) -> Result<i32, Errno> {
    let pid = pid as usize;
    syscall1(SYS_SCHED_GETSCHEDULER, pid).map(|ret| ret as i32)
}
