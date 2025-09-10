/// Get file system statistics
pub unsafe fn fstatvfs1(fd: i32, buf: &mut statvfs_t, flags: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let buf_ptr = buf as *mut statvfs_t as usize;
    let flags = flags as usize;
    syscall3(SYS_FSTATVFS1, fd, buf_ptr, flags).map(drop)
}
