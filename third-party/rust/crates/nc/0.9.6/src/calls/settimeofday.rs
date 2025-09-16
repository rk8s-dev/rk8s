/// Set system time and timezone.
///
/// ```
/// let tv = nc::timeval_t {
///     tv_sec: 1,
///     tv_usec: 0,
/// };
/// let ret = unsafe { nc::settimeofday(&tv, None) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn settimeofday(timeval: &timeval_t, tz: Option<&timezone_t>) -> Result<(), Errno> {
    let timeval_ptr = timeval as *const timeval_t as usize;
    let tz_ptr = tz.map_or(core::ptr::null::<timezone_t>() as usize, |tz| {
        tz as *const timezone_t as usize
    });
    syscall2(SYS_SETTIMEOFDAY, timeval_ptr, tz_ptr).map(drop)
}
