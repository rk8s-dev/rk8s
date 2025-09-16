/// Set system time and timezone.
pub unsafe fn __settimeofday50(timeval: &timeval_t, tz: &timezone_t) -> Result<(), Errno> {
    let timeval_ptr = timeval as *const timeval_t as usize;
    let tz_ptr = tz as *const timezone_t as usize;
    syscall2(SYS___SETTIMEOFDAY50, timeval_ptr, tz_ptr).map(drop)
}
