/// Transfer data between file descriptors.
pub unsafe fn sendfile64(
    out_fd: i32,
    in_fd: i32,
    offset: loff_t,
    count: size_t,
) -> Result<ssize_t, Errno> {
    let out_fd = out_fd as usize;
    let in_fd = in_fd as usize;
    let offset = offset as usize;
    syscall4(SYS_SENDFILE64, out_fd, in_fd, offset, count).map(|ret| ret as ssize_t)
}
