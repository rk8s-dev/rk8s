/// Change timestamp of a file.
pub unsafe fn futimes(fd: i32, times: &[timeval_t; 2]) -> Result<(), Errno> {
    let fd = fd as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS_FUTIMES, fd, times_ptr).map(drop)
}
