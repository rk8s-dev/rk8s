/// Send a signal to a thread (obsolete).
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
///     let ret = unsafe { nc::tkill(pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn tkill(tid: i32, sig: i32) -> Result<(), Errno> {
    let tid = tid as usize;
    let sig = sig as usize;
    syscall2(SYS_TKILL, tid, sig).map(drop)
}
