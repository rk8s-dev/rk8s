/// Make a new name for a file.
///
/// # Examples
///
/// ```
/// let oldname = "/etc/passwd";
/// let newname = "/tmp/nc-symlink";
/// let ret = unsafe { nc::symlink(oldname, newname) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, newname,0 ) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn symlink<P: AsRef<Path>>(old_name: P, new_name: P) -> Result<(), Errno> {
    let old_name = CString::new(old_name.as_ref());
    let old_name_ptr = old_name.as_ptr() as usize;
    let new_name = CString::new(new_name.as_ref());
    let new_name_ptr = new_name.as_ptr() as usize;
    syscall2(SYS_SYMLINK, old_name_ptr, new_name_ptr).map(drop)
}
