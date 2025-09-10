/// Get the process ID (PID) in the process descriptor fd.
pub unsafe fn pdgetpid(fd: i32, pid: &mut pid_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let pid_ptr = pid as *mut pid_t as usize;
    syscall2(SYS_PDGETPID, fd, pid_ptr).map(drop)
}
