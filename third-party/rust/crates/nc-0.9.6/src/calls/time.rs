/// Get time in seconds.
///
/// # Examples
///
/// ```
/// let mut t = 0;
/// let ret = unsafe { nc::time(&mut t) };
/// assert_eq!(ret.unwrap(), t);
/// assert!(t > 1610421040);
/// ```
pub unsafe fn time(t: &mut time_t) -> Result<time_t, Errno> {
    syscall1(SYS_TIME, t as *mut time_t as usize).map(|ret| ret as time_t)
}
