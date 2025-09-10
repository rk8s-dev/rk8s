/// Signal handling.
///
/// Deprecated. Use `sigaction()` instead.
///
/// # Examples
///
/// ```
/// fn handle_sigterm(signum: i32) {
///     assert_eq!(signum, nc::SIGTERM);
/// }
/// let ret = unsafe { nc::signal(nc::SIGTERM, handle_sigterm as nc::sighandler_t) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::kill(nc::getpid(), nc::SIGTERM) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn signal(sig: i32, handler: sighandler_t) -> Result<sighandler_t, Errno> {
    let sig = sig as usize;
    syscall2(SYS_SIGNAL, sig, handler).map(|ret| ret as sighandler_t)
}
