/// Open a pseudo-terminal device
pub unsafe fn posix_openpt(flags: i32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_POSIX_OPENPT, flags).map(|ret| ret as i32)
}
