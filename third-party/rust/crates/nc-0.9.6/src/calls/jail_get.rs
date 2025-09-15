/// Retrieves jail parameters.
pub unsafe fn jail_get(iov: &mut [iovec_t], flags: i32) -> Result<i32, Errno> {
    let iov_ptr = iov.as_mut_ptr() as usize;
    let iov_len = iov.len();
    let flags = flags as usize;
    syscall3(SYS_JAIL_GET, iov_ptr, iov_len, flags).map(|val| val as i32)
}
