/// Queue a signal to a process (REALTIME)
pub unsafe fn sigqueue(pid: pid_t, signum: i32, value: usize) -> Result<(), Errno> {
    let pid = pid as usize;
    let signum = signum as usize;
    syscall3(SYS_SIGQUEUE, pid, signum, value).map(drop)
}
