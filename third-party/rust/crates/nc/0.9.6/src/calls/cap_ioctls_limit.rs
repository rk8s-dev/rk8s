/// Reduce the list of allowed `ioctl()` commands if a file descriptor
/// is granted the CAP_IOCTL capability right,
pub unsafe fn cap_ioctls_limit(fd: i32, cmds: &[usize]) -> Result<(), Errno> {
    let fd = fd as usize;
    let cmds_ptr = cmds.as_ptr() as usize;
    let ncmds = cmds.len();
    syscall3(SYS_CAP_IOCTLS_LIMIT, fd, cmds_ptr, ncmds).map(drop)
}
