/// Set an alarm clock for delivery of a signal.
///
/// # Examples
///
/// ```
/// fn handle_alarm(signum: i32) {
///     assert_eq!(signum, nc::SIGALRM);
/// }
///
/// let sa = nc::new_sigaction(handle_alarm);
/// let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
/// assert!(ret.is_ok());
/// let remaining = unsafe { nc::alarm(1) };
/// let mask = nc::sigset_t::default();
/// let ret = unsafe { nc::rt_sigsuspend(&mask) };
/// assert_eq!(ret, Err(nc::EINTR));
/// assert_eq!(remaining, Ok(0));
/// ```
pub unsafe fn alarm(seconds: u32) -> Result<u32, Errno> {
    let seconds = seconds as usize;
    syscall1(SYS_ALARM, seconds).map(|ret| ret as u32)
}
