/// Get time.
pub unsafe fn __gettimeofday590(timeval: &mut timeval_t, tz: &mut timezone_t) -> Result<(), Errno> {
    let timeval_ptr = timeval as *mut timeval_t as usize;
    let tz_ptr = tz as *mut timezone_t as usize;
    syscall2(SYS___GETTIMEOFDAY50, timeval_ptr, tz_ptr).map(drop)
}
