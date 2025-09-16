/// Create a file descriptor to handle page faults in user space.
pub unsafe fn userfaultfd(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_USERFAULTFD, flags).map(|ret| ret as i32)
}
