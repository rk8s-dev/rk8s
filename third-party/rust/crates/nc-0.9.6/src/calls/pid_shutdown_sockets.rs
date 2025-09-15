pub unsafe fn pid_shutdown_sockets(pid: pid_t, level: i32) -> Result<(), Errno> {
    let pid = pid as usize;
    let level = level as usize;
    syscall2(SYS_PID_SHUTDOWN_SOCKETS, pid, level).map(drop)
}
