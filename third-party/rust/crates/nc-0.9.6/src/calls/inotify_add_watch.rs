/// Add a watch to an initialized inotify instance.
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
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn inotify_add_watch<P: AsRef<Path>>(
    fd: i32,
    filename: P,
    mask: u32,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mask = mask as usize;
    syscall3(SYS_INOTIFY_ADD_WATCH, fd, filename_ptr, mask).map(|ret| ret as i32)
}
