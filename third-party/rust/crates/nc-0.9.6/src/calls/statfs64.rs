/// Get filesystem statistics.
///
/// # Examples
///
/// ```
/// let path = "/usr";
/// let mut statfs = nc::statfs64_t::default();
/// let ret = unsafe { nc::statfs64(path, &mut statfs) };
/// assert!(ret.is_ok());
/// assert!(statfs.f_bfree > 0);
/// assert!(statfs.f_bavail > 0);
/// ```
pub unsafe fn statfs64<P: AsRef<Path>>(filename: P, buf: &mut statfs64_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let buf_ptr = buf as *mut statfs64_t as usize;
    syscall2(SYS_STATFS64, filename_ptr, buf_ptr).map(drop)
}
