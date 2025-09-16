/// Change name or location of a file.
///
/// # Examples
///
/// ```
/// let path = "/tmp/nc-renameat2";
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::close(fd) };
/// assert!(ret.is_ok());
/// let new_path = "/tmp/nc-renameat2-new";
/// let flags = nc::RENAME_NOREPLACE;
/// let ret = unsafe { nc::renameat2(nc::AT_FDCWD, path, nc::AT_FDCWD, new_path, flags) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, new_path, 0) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn renameat2<P: AsRef<Path>>(
    olddfd: i32,
    oldfilename: P,
    newdfd: i32,
    newfilename: P,
    flags: i32,
) -> Result<(), Errno> {
    let olddfd = olddfd as usize;
    let oldfilename = CString::new(oldfilename.as_ref());
    let oldfilename_ptr = oldfilename.as_ptr() as usize;
    let newdfd = newdfd as usize;
    let newfilename = CString::new(newfilename.as_ref());
    let newfilename_ptr = newfilename.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(
        SYS_RENAMEAT2,
        olddfd,
        oldfilename_ptr,
        newdfd,
        newfilename_ptr,
        flags,
    )
    .map(drop)
}
