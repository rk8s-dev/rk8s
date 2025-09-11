/// Set time of specific clock.
///
/// # Examples
///
/// ```
/// let mut tp = nc::timespec_t::default();
/// let ret = unsafe { nc::clock_gettime(nc::CLOCK_REALTIME, &mut tp) };
/// assert!(ret.is_ok());
/// assert!(tp.tv_sec > 0);
/// let ret = unsafe { nc::clock_settime(nc::CLOCK_REALTIME, &tp) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn clock_settime(which_clock: clockid_t, tp: &timespec_t) -> Result<(), Errno> {
    let which_clock = which_clock as usize;
    let tp_ptr = tp as *const timespec_t as usize;
    syscall2(SYS_CLOCK_SETTIME, which_clock, tp_ptr).map(drop)
}
