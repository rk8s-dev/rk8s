/// Close all file descriptors in a given range
///
/// Parameters:
/// - `fd`: starting file descriptor to close
/// - `max_fd`: last file descriptor to close
/// - `flags`: reserved for future extensions
///
/// # Examples
///
/// ```
/// const STDOUT_FD: u32 = 1;
/// const STDERR_FD: u32 = 2;
/// let ret = unsafe { nc::close_range(STDOUT_FD, STDERR_FD, nc::CLOSE_RANGE_CLOEXEC) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn close_range(fd: u32, max_fd: u32, flags: u32) -> Result<(), Errno> {
    let fd = fd as usize;
    let max_fd = max_fd as usize;
    let flags = flags as usize;
    syscall3(SYS_CLOSE_RANGE, fd, max_fd, flags).map(drop)
}
