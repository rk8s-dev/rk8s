/// Get current working directory.
///
/// # Examples
///
/// ```
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::__getcwd(&mut buf) };
/// assert!(ret.is_ok());
/// // Remove null-terminal char.
/// let path_len = ret.unwrap() as usize - 1;
/// let cwd = std::str::from_utf8(&buf[..path_len]);
/// assert!(cwd.is_ok());
/// ```
pub unsafe fn __getcwd(buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let buf_ptr = buf.as_mut_ptr() as usize;
    let size = buf.len();
    syscall2(SYS___GETCWD, buf_ptr, size).map(|ret| ret as ssize_t)
}
