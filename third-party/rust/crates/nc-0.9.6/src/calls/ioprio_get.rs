/// Get I/O scheduling class and priority.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::ioprio_get(nc::IOPRIO_WHO_PROCESS, nc::getpid()) };
/// assert!(ret.is_ok());
/// let prio = ret.unwrap();
/// let _prio_class = unsafe { nc::ioprio_prio_class(prio) };
/// let _prio_data = unsafe { nc::ioprio_prio_data(prio) };
/// ```
pub unsafe fn ioprio_get(which: i32, who: i32) -> Result<i32, Errno> {
    let which = which as usize;
    let who = who as usize;
    syscall2(SYS_IOPRIO_GET, which, who).map(|ret| ret as i32)
}
