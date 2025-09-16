/// Get real, effect and saved user ID.
///
/// # Examples
///
/// ```
/// let mut ruid = 0;
/// let mut euid = 0;
/// let mut suid = 0;
/// let ret = unsafe { nc::getresuid(&mut ruid, &mut euid, &mut suid) };
/// assert!(ret.is_ok());
/// assert!(ruid > 0);
/// assert!(euid > 0);
/// assert!(suid > 0);
/// ```
pub unsafe fn getresuid(ruid: &mut uid_t, euid: &mut uid_t, suid: &mut uid_t) -> Result<(), Errno> {
    let ruid_ptr = ruid as *mut uid_t as usize;
    let euid_ptr = euid as *mut uid_t as usize;
    let suid_ptr = suid as *mut uid_t as usize;
    syscall3(SYS_GETRESUID, ruid_ptr, euid_ptr, suid_ptr).map(drop)
}
