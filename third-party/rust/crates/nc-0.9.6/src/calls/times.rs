/// Get process times.
///
/// # Examples
///
/// ```
/// let mut tms = nc::tms_t::default();
/// let ret = unsafe { nc::times(&mut tms) };
/// assert!(ret.is_ok());
/// let clock = ret.unwrap();
/// assert!(clock > 0);
/// ```
pub unsafe fn times(buf: &mut tms_t) -> Result<clock_t, Errno> {
    let buf_ptr = buf as *mut tms_t as usize;
    syscall1(SYS_TIMES, buf_ptr).map(|ret| ret as clock_t)
}
