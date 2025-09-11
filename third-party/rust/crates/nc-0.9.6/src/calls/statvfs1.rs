/// Get file system statistics
pub unsafe fn statvfs1<P: AsRef<Path>>(
    path: P,
    buf: &mut statvfs_t,
    flags: i32,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall3(SYS_STATVFS1, path_ptr, buf_ptr, flags).map(drop)
}
