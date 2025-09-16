/// Set value of an interval timer.
///
/// # Examples
///
/// ```
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, nc::SIGALRM);
///     let msg = b"Hello alarm\n";
///     let stderr = 2;
///     let _ = unsafe { nc::write(stderr, msg) };
/// }
///
/// let sa = nc::new_sigaction(handle_alarm);
/// let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
/// assert!(ret.is_ok());
///
/// // Single shot timer, actived after 1 second.
/// let itv = nc::itimerval_t {
///     it_value: nc::timeval_t {
///         tv_sec: 1,
///         tv_usec: 0,
///     },
///     it_interval: nc::timeval_t {
///         tv_sec: 0,
///         tv_usec: 0,
///     },
/// };
/// let ret = unsafe { nc::setitimer(nc::ITIMER_REAL, &itv, None) };
/// assert!(ret.is_ok());
///
/// let mut prev_itv = nc::itimerval_t::default();
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert!(prev_itv.it_value.tv_sec <= itv.it_value.tv_sec);
///
/// let mask = nc::sigset_t::default();
/// let _ret = unsafe { nc::rt_sigsuspend(&mask) };
///
/// let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
/// assert!(ret.is_ok());
/// assert_eq!(prev_itv.it_value.tv_sec, 0);
/// assert_eq!(prev_itv.it_value.tv_usec, 0);
/// ```
pub unsafe fn setitimer(
    which: i32,
    new_val: &itimerval_t,
    old_val: Option<&mut itimerval_t>,
) -> Result<(), Errno> {
    let which = which as usize;
    let new_val_ptr = new_val as *const itimerval_t as usize;
    let old_val_ptr = old_val.map_or(core::ptr::null_mut::<itimerval_t>() as usize, |old_val| {
        old_val as *mut itimerval_t as usize
    });
    syscall3(SYS_SETITIMER, which, new_val_ptr, old_val_ptr).map(drop)
}
