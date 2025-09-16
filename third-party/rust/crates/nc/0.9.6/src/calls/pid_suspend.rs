pub unsafe fn pid_suspend(pid: pid_t) -> Result<(), Errno> {
    let pid = pid as usize;
    syscall1(SYS_PID_SUSPEND, pid).map(drop)
}
