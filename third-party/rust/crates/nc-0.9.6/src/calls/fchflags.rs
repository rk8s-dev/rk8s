/// Set file flags.
pub unsafe fn fchflags(fd: i32, flags: fflags_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let flags = flags as usize;
    syscall2(SYS_FCHFLAGS, fd, flags).map(drop)
}
