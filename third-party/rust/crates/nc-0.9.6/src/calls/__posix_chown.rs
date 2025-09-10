/// Change ownership of a file.
pub unsafe fn __posix_chown<P: AsRef<Path>>(
    filename: P,
    user: uid_t,
    group: gid_t,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS___POSIX_CHOWN, filename_ptr, user, group).map(drop)
}
