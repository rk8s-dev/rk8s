/// Set group identify used for filesystem checkes.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setfsgid(0) };
/// assert!(ret.is_ok());
/// let gid = unsafe { nc::getgid() };
/// assert_eq!(ret, Ok(gid));
/// ```
pub unsafe fn setfsgid(fsgid: gid_t) -> Result<gid_t, Errno> {
    let fsgid = fsgid as usize;
    syscall1(SYS_SETFSGID, fsgid).map(|ret| ret as gid_t)
}
