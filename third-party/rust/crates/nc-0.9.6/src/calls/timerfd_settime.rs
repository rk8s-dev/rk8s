/// Set current timer via a file descriptor.
///
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
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn timerfd_settime(
    ufd: i32,
    flags: i32,
    new_value: &itimerspec_t,
    old_value: Option<&mut itimerspec_t>,
) -> Result<(), Errno> {
    let ufd = ufd as usize;
    let flags = flags as usize;
    let new_value_ptr = new_value as *const itimerspec_t as usize;
    let old_value_ptr = old_value.map_or(
        core::ptr::null_mut::<itimerspec_t>() as usize,
        |old_value| old_value as *mut itimerspec_t as usize,
    );
    syscall4(
        SYS_TIMERFD_SETTIME,
        ufd,
        flags,
        new_value_ptr,
        old_value_ptr,
    )
    .map(drop)
}
