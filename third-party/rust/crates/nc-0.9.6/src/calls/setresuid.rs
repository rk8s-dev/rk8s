/// Set real, effective and saved user Ids of the calling process.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::setresuid(0, 0, 0) };
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn setresuid(ruid: uid_t, euid: uid_t, suid: uid_t) -> Result<(), Errno> {
    let ruid = ruid as usize;
    let euid = euid as usize;
    let suid = suid as usize;
    syscall3(SYS_SETRESUID, ruid, euid, suid).map(drop)
}
