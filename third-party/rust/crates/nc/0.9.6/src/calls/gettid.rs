/// Get the caller's thread ID (TID).
///
/// # Examples
///
/// ```
/// let tid = unsafe { nc::gettid() };
/// assert!(tid > 0);
/// ```
#[must_use]
pub unsafe fn gettid() -> pid_t {
    // This function is always successful.
    syscall0(SYS_GETTID).unwrap_or_default() as pid_t
}
