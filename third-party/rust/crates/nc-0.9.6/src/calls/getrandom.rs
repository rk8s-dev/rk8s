/// Obtain a series of random bytes.
///
/// # Examples
///
/// ```
/// let mut buf = [0_u8; 32];
/// let ret = unsafe { nc::getrandom(&mut buf, 0) };
/// assert!(ret.is_ok());
/// let size = ret.unwrap() as usize;
/// assert!(size <= buf.len());
/// ```
pub unsafe fn getrandom(buf: &mut [u8], flags: u32) -> Result<ssize_t, Errno> {
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    let flags = flags as usize;
    syscall3(SYS_GETRANDOM, buf_ptr, buf_len, flags).map(|ret| ret as ssize_t)
}
