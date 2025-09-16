/// Change timestamp of a file.
pub unsafe fn __futimes50(fd: i32, times: &[timeval_t; 2]) -> Result<(), Errno> {
    let fd = fd as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS___FUTIMES50, fd, times_ptr).map(drop)
}
