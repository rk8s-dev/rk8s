/// Pre-allocate storage for a range in a file
pub unsafe fn posix_fallocate(fd: i32, offset: off_t, len: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let len = len as usize;
    syscall3(SYS_POSIX_FALLOCATE, fd, offset, len).map(drop)
}
