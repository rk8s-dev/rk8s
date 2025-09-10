/// Get the real user ID of the calling process.
///
/// # Examples
///
/// ```
/// let uid = unsafe { nc::getuid() };
/// assert!(uid > 0);
/// ```
#[must_use]
pub unsafe fn getuid() -> uid_t {
    // This function is always successful.
    syscall0(SYS_GETUID).unwrap_or_default() as uid_t
}
