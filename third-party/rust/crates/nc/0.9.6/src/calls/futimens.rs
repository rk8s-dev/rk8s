/// Change timestamp of a file.
pub unsafe fn futimens(fd: i32, times: &[timespec_t; 2]) -> Result<(), Errno> {
    let fd = fd as usize;
    let times_ptr = times.as_ptr() as usize;
    syscall2(SYS_FUTIMENS, fd, times_ptr).map(drop)
}
