/// Get configurable pathname variables
pub unsafe fn fpathconf(fd: i32, name: i32) -> Result<isize, Errno> {
    let fd = fd as usize;
    let name = name as usize;
    syscall2(SYS_FPATHCONF, fd, name).map(|val| val as isize)
}
