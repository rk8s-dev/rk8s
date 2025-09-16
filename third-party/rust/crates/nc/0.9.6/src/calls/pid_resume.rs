pub unsafe fn pid_resume(pid: pid_t) -> Result<(), Errno> {
    let pid = pid as usize;
    syscall1(SYS_PID_RESUME, pid).map(drop)
}
