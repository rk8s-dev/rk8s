/// Open an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let poll_fd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(poll_fd.is_ok());
/// let poll_fd = poll_fd.unwrap();
/// let ret = unsafe { nc::close(poll_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_create1(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_EPOLL_CREATE1, flags).map(|ret| ret as i32)
}
