/// Get file status about a file.
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let mut stat = nc::stat64_t::default();
/// let ret = unsafe { nc::stat64(path, &mut stat) };
/// assert!(ret.is_ok());
/// // Check fd is a regular file.
/// assert!(nc::S_ISREG(stat.st_mode as nc::mode_t));
/// ```
pub unsafe fn stat64<P: AsRef<Path>>(filename: P, statbuf: &mut stat64_t) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let statbuf_ptr = statbuf as *mut stat64_t as usize;
    syscall2(SYS_STAT64, filename_ptr, statbuf_ptr).map(drop)
}
