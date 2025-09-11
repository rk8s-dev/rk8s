/// Wait for a signal.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
/// assert!(pid.is_ok());
/// let pid = pid.unwrap();
/// assert!(pid >= 0);
///
/// if pid == 0 {
///     // child process.
///     let mask = nc::old_sigset_t::default();
///     let ret = unsafe { nc::sigsuspend(&mask) };
///     assert!(ret.is_ok());
/// } else {
///     // parent process.
///     let t = nc::timespec_t {
///         tv_sec: 1,
///         tv_nsec: 0,
///     };
///     let ret = unsafe { nc::nanosleep(&t, None) };
///     assert!(ret.is_ok());
///
///     let ret = unsafe { nc::kill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn sigsuspend(mask: &old_sigset_t) -> Result<(), Errno> {
    let mask_ptr = mask as *const old_sigset_t as usize;
    syscall1(SYS_SIGSUSPEND, mask_ptr).map(drop)
}
