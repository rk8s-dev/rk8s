/// Manipulate process resources
pub unsafe fn rfork(flags: i32) -> Result<pid_t, Errno> {
    let flags = flags as usize;
    syscall1(SYS_RFORK, flags).map(|ret| ret as pid_t)
}
