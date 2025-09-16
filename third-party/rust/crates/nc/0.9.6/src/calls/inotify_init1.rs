/// Initialize an inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init1(nc::IN_NONBLOCK | nc::IN_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_init1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_INOTIFY_INIT1, flags).map(|ret| ret as i32)
}
