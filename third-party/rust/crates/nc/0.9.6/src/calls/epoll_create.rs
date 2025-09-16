/// Open an epoll file descriptor.
///
/// Note that `size` is ignored.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::epoll_create(32) };
/// assert!(ret.is_ok());
/// let poll_fd = ret.unwrap();
/// let ret = unsafe { nc::close(poll_fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_create(size: i32) -> Result<i32, Errno> {
    let size = size as usize;
    syscall1(SYS_EPOLL_CREATE, size).map(|ret| ret as i32)
}
