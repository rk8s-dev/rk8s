/// Read and/or clear kernel message ring buffer.
///
/// # Examples
///
/// ```
/// let mut buf = vec![0_u8; 4096];
/// let ret = unsafe { nc::syslog(nc::SYSLOG_ACTION_READ_ALL, &mut buf) };
/// if let Ok(nread) = ret {
///     if let Ok(msg) = std::str::from_utf8(&buf[..nread as usize]) {
///         println!("msg: {msg}");
///     }
/// } else {
///     assert_eq!(ret, Err(nc::EPERM));
/// }
/// ```
pub unsafe fn syslog(action: i32, buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let action = action as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS_SYSLOG, action, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}
