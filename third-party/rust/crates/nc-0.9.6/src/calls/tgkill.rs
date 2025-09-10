/// Send a signal to a thread.
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
///     let ret = unsafe { nc::tgkill(pid, pid, nc::SIGTERM) };
///     assert!(ret.is_ok());
/// }
/// ```
pub unsafe fn tgkill(tgid: i32, tid: i32, sig: i32) -> Result<(), Errno> {
    let tgid = tgid as usize;
    let tid = tid as usize;
    let sig = sig as usize;
    syscall3(SYS_TGKILL, tgid, tid, sig).map(drop)
}
