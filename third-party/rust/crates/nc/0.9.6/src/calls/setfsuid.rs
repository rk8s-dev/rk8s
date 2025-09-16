/// Set user identify used for filesystem checkes.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setfsuid(0) };
/// assert!(ret.is_ok());
/// let uid = unsafe { nc::getuid() };
/// assert_eq!(ret, Ok(uid));
/// ```
pub unsafe fn setfsuid(fsuid: uid_t) -> Result<uid_t, Errno> {
    let fsuid = fsuid as usize;
    syscall1(SYS_SETFSUID, fsuid).map(|ret| ret as uid_t)
}
