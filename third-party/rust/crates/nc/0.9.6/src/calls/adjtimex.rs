/// Tune kernel clock. Returns clock state on success.
///
/// # Examples
///
/// ```
/// let mut tm = nc::timex_t::default();
/// let ret = unsafe { nc::adjtimex(&mut tm) };
/// assert!(ret.is_ok());
/// assert!(tm.time.tv_sec > 1611552896);
/// ```
pub unsafe fn adjtimex(buf: &mut timex_t) -> Result<i32, Errno> {
    let buf_ptr = buf as *mut timex_t as usize;
    syscall1(SYS_ADJTIMEX, buf_ptr).map(|ret| ret as i32)
}
