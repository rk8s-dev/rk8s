/// Discard backing store for files
pub unsafe fn fdiscard(fd: i32, offset: off_t, len: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let len = len as usize;
    syscall3(SYS_FDISCARD, fd, offset, len).map(drop)
}
