/// Return system information.
///
/// # Examples
///
/// ```
/// let mut info = nc::sysinfo_t::default();
/// let ret = unsafe { nc::sysinfo(&mut info) };
/// assert!(ret.is_ok());
/// assert!(info.uptime > 0);
/// assert!(info.freeram > 0);
/// ```
pub unsafe fn sysinfo(info: &mut sysinfo_t) -> Result<(), Errno> {
    let info_ptr = info as *mut sysinfo_t as usize;
    syscall1(SYS_SYSINFO, info_ptr).map(drop)
}
