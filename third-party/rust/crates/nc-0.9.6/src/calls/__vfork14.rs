/// Create a child process and wait until it is terminated.
pub unsafe fn __vfork14() -> Result<pid_t, Errno> {
    syscall0(SYS___VFORK14).map(|ret| ret as pid_t)
}
