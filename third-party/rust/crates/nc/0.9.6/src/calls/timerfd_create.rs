/// Create a timer that notifies via a file descriptor.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::timerfd_create(nc::CLOCK_MONOTONIC, nc::TFD_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_create(clockid: i32, flags: i32) -> Result<i32, Errno> {
    let clockid = clockid as usize;
    let flags = flags as usize;
    syscall2(SYS_TIMERFD_CREATE, clockid, flags).map(|ret| ret as i32)
}
