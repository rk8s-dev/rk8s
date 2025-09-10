/// Get the list of allowed `ioctl()` commands if a file descriptor
/// is granted the CAP_IOCTL capability right,
pub unsafe fn cap_ioctls_get(fd: i32, cmds: &mut [usize]) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let cmds_ptr = cmds.as_mut_ptr() as usize;
    let max_cmds = cmds.len();
    syscall3(SYS_CAP_IOCTLS_GET, fd, cmds_ptr, max_cmds).map(|val| val as ssize_t)
}
