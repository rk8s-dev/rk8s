/// Set real and effective user IDs of the calling process.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setreuid(0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setreuid(ruid: uid_t, euid: uid_t) -> Result<(), Errno> {
    let ruid = ruid as usize;
    let euid = euid as usize;
    syscall2(SYS_SETREUID, ruid, euid).map(drop)
}
