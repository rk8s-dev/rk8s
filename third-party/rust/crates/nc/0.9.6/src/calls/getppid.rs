/// Get the process ID of the parent of the calling process.
///
/// # Examples
///
/// ```
/// let ppid = unsafe { nc::getppid() };
/// assert!(ppid > 0);
/// ```
#[must_use]
pub unsafe fn getppid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETPPID).unwrap_or_default() as pid_t
}
