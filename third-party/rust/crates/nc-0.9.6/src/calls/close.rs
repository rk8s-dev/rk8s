/// Close a file descriptor.
///
/// # Examples
///
/// ```
/// const STDERR_FD: i32 = 2;
/// let ret = unsafe { nc::close(STDERR_FD) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn close(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_CLOSE, fd).map(drop)
}
