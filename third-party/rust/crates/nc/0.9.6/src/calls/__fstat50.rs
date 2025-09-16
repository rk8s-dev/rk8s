/// Get file status about a file descriptor.
pub unsafe fn __fstat50(fd: i32, statbuf: &mut stat_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let statbuf_ptr = statbuf as *mut stat_t as usize;
    syscall2(SYS___FSTAT50, fd, statbuf_ptr).map(drop)
}
