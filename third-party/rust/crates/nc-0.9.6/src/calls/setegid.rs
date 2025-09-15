/// Set the effective group ID of the calling process to `gid`.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setegid(0) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setegid(gid: gid_t) -> Result<(), Errno> {
    let gid = gid as usize;
    syscall1(SYS_SETEGID, gid).map(drop)
}
