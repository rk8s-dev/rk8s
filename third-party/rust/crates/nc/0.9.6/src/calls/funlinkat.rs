/// Delete a name and possibly the file it refers to.
pub unsafe fn funlinkat<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    fd: i32,
    flag: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let fd = fd as usize;
    let flag = flag as usize;
    syscall4(SYS_FUNLINKAT, dfd, filename_ptr, fd, flag).map(drop)
}
