/// Get the process ID (PID) of the calling process.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::getpid() };
/// assert!(pid > 0);
/// ```
#[must_use]
pub unsafe fn getpid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPID).unwrap_or_default() as pid_t
}
