/// Get the effective group ID of the calling process.
///
/// # Examples
///
/// ```
/// let egid = unsafe { nc::getegid() };
/// assert!(egid > 0);
/// ```
#[must_use]
pub unsafe fn getegid() -> gid_t {
    // This function is always successful.
    syscall0(SYS_GETEGID).unwrap_or_default() as gid_t
}
