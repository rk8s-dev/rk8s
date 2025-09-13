/// Mount filesystem.
pub unsafe fn nmount(iov: &mut [iovec_t], flags: i32) -> Result<(), Errno> {
    let iov_ptr = iov.as_mut_ptr() as usize;
    let iov_len = iov.len();
    let flags = flags as usize;
    syscall3(SYS_NMOUNT, iov_ptr, iov_len, flags).map(drop)
}
