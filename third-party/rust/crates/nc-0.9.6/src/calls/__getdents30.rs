/// Get directory entries.
pub unsafe fn getdents(fd: i32, dirp: usize, count: size_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    syscall3(SYS___GETDENTS30, fd, dirp, count).map(|ret| ret as ssize_t)
}
