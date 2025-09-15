/// Make process 0 idle.
///
/// Never returns for process 0, and already returns EPERM for a user process.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::idle() };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn idle() -> Result<(), Errno> {
    syscall0(SYS_IDLE).map(drop)
}
