/// Get the real group ID of the calling process.
///
/// # Examples
///
/// ```
/// let gid = unsafe { nc::getgid() };
/// assert!(gid > 0);
/// ```
#[must_use]
pub unsafe fn getgid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETGID).unwrap_or_default() as gid_t
}
