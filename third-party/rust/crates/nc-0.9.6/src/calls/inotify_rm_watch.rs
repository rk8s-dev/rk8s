/// Remove an existing watch from an inotify instance.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::inotify_init1(nc::IN_NONBLOCK | nc::IN_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let path = "/etc/passwd";
/// let ret = unsafe { nc::inotify_add_watch(fd, path, nc::IN_MODIFY) };
/// assert!(ret.is_ok());
/// let wd = ret.unwrap();
/// let ret = unsafe { nc::inotify_rm_watch(fd, wd) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_rm_watch(fd: i32, wd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let wd = wd as usize;
    syscall2(SYS_INOTIFY_RM_WATCH, fd, wd).map(drop)
}
