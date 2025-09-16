/// Wait for a signal.
///
/// Always returns `Errno`, normally `EINTR`.
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
///     let mask = nc::sigset_t::default();
///     let ret = unsafe { nc::rt_sigsuspend(&mask) };
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
pub unsafe fn rt_sigsuspend(set: &sigset_t) -> Result<(), Errno> {
    let set_ptr = set as *const sigset_t as usize;
    let sigset_size = core::mem::size_of::<sigset_t>();
    syscall2(SYS_RT_SIGSUSPEND, set_ptr, sigset_size).map(drop)
}
