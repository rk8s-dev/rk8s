/// Get name and information about current kernel.
///
/// # Examples
///
/// ```
/// let mut buf = nc::utsname_t::default();
/// let ret = unsafe { nc::uname(&mut buf) };
/// assert!(ret.is_ok());
/// assert!(!buf.sysname.is_empty());
/// assert!(!buf.machine.is_empty());
/// ```
pub unsafe fn uname(buf: &mut utsname_t) -> Result<(), Errno> {
    let buf_ptr = buf as *mut utsname_t as usize;
    syscall1(SYS_UNAME, buf_ptr).map(drop)
}
