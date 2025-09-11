/// Reduce the list of allowed `fcntl()` commands if a file descriptor
/// is granted the CAP_IOCTL capability right,
pub unsafe fn cap_fcntls_limit(fd: i32, fcntl_rights: u32) -> Result<(), Errno> {
    let fd = fd as usize;
    let fcntl_rights = fcntl_rights as usize;
    syscall2(SYS_CAP_FCNTLS_LIMIT, fd, fcntl_rights).map(drop)
}
