/// Set the effective user ID of the calling process to `uid`.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::seteuid(0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn seteuid(uid: uid_t) -> Result<(), Errno> {
    let uid = uid as usize;
    syscall1(SYS_SETEUID, uid).map(drop)
}
