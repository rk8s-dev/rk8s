/// Create a child process and returns a process descriptor.
pub unsafe fn pdfork(fd: &mut i32, flags: i32) -> Result<pid_t, Errno> {
    let fd_ptr = fd as *mut i32 as usize;
    let flags = flags as usize;
    syscall2(SYS_PDFORK, fd_ptr, flags).map(|ret| ret as pid_t)
}
