/// Restart a system call after interruption by a stop signal.
pub unsafe fn restart_syscall() -> Result<i32, Errno> {
    syscall0(SYS_RESTART_SYSCALL).map(|ret| ret as i32)
}
