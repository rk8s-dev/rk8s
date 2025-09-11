/// Create a child process.
pub unsafe fn __clone(flags: i32, stack: uintptr_t) -> Result<pid_t, Errno> {
    let flags = flags as usize;
    syscall2(SYS___CLONE, flags, stack).map(|ret| ret as pid_t)
}
