/// Change I/O privilege level.
///
/// # Examples
///
/// ```
/// let ret = unsafe { nc::iopl(1) };
/// assert!(ret.is_err());
/// assert_eq!(ret, Err(nc::EPERM));
/// ```
pub unsafe fn iopl(level: i32) -> Result<(), Errno> {
    let level = level as usize;
    syscall1(SYS_IOPL, level).map(drop)
}
