/// Open and possibly create a file (extended)
///
/// # Examples
///
/// ```
/// let path = "/etc/passwd";
/// let mut how = nc::open_how_t{
///   flags: nc::O_RDONLY as u64,
///   ..Default::default()
/// };
/// let ret = unsafe { nc::openat2(nc::AT_FDCWD, path, &mut how) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn openat2<P: AsRef<Path>>(
    dirfd: i32,
    pathname: P,
    how: &mut open_how_t,
) -> Result<i32, Errno> {
    let dirfd = dirfd as usize;
    let pathname = CString::new(pathname.as_ref());
    let pathname_ptr = pathname.as_ptr() as usize;
    let how_ptr = how as *mut open_how_t as usize;
    let how_size = core::mem::size_of::<open_how_t>();
    syscall4(SYS_OPENAT2, dirfd, pathname_ptr, how_ptr, how_size).map(|ret| ret as i32)
}
