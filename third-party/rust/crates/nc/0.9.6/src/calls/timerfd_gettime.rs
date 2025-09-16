/// Get current timer via a file descriptor.
/// # Examples
///
/// ```
/// let ret = unsafe { nc::timerfd_create(nc::CLOCK_MONOTONIC, nc::TFD_CLOEXEC) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
///
/// let flags = 0;
/// let time = nc::itimerspec_t {
///     it_interval: nc::timespec_t::default(),
///     it_value: nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     },
/// };
/// let ret = unsafe { nc::timerfd_settime(fd, flags, &time, None) };
/// assert!(ret.is_ok());
///
/// let mut curr_time = nc::itimerspec_t::default();
/// let ret = unsafe { nc::timerfd_gettime(fd, &mut curr_time) };
/// assert!(ret.is_ok());
/// println!("curr time: {curr_time:?}");
///
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_gettime(ufd: i32, cur_value: &mut itimerspec_t) -> Result<(), Errno> {
    let ufd = ufd as usize;
    let cur_value_ptr = cur_value as *mut itimerspec_t as usize;
    syscall2(SYS_TIMERFD_GETTIME, ufd, cur_value_ptr).map(drop)
}
