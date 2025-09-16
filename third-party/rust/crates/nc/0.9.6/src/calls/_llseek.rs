/// Reposition read/write file offset.
pub unsafe fn _llseek(
    fd: i32,
    offset_high: usize,
    offset_low: usize,
    result: &mut loff_t,
    whence: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let result_ptr = result as *mut loff_t as usize;
    let whence = whence as usize;
    syscall5(SYS__LLSEEK, fd, offset_high, offset_low, result_ptr, whence).map(drop)
}
