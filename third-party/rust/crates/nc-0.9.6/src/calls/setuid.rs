/// Set user ID of the calling process to `uid`.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setuid(0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setuid(uid: uid_t) -> Result<(), Errno> {
    let uid = uid as usize;
    syscall1(SYS_SETUID, uid).map(drop)
}
