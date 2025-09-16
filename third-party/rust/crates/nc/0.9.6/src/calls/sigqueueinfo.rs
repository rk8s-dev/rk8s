/// Queue a signal to a process (REALTIME)
pub unsafe fn sigqueueinfo(pid: pid_t, info: &siginfo_t) -> Result<(), Errno> {
    let pid = pid as usize;
    let info_ptr = info as *const siginfo_t as usize;
    syscall2(SYS_SIGQUEUEINFO, pid, info_ptr).map(drop)
}
