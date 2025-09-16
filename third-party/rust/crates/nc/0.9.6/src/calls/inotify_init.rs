/// Initialize an inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init() };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_init() -> Result<i32, Errno> {
    syscall0(SYS_INOTIFY_INIT).map(|ret| ret as i32)
}
