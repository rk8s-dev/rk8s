/// Change working directory.
///
/// # Examples
///
/// ```
/// let path = "/tmp";
/// // Open folder directly.
/// let ret = unsafe { nc::chdir(path) };
/// assert!(ret.is_ok());
///
/// let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
/// let ret = unsafe { nc::getcwd(&mut buf) };
/// assert!(ret.is_ok());
/// // Remove null-terminal char.
/// let path_len = ret.unwrap() as usize - 1;
/// let new_cwd = std::str::from_utf8(&buf[..path_len]);
/// assert_eq!(new_cwd, Ok(path));
/// ```
pub unsafe fn chdir<P: AsRef<Path>>(filename: P) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    syscall1(SYS_CHDIR, filename_ptr).map(drop)
}
