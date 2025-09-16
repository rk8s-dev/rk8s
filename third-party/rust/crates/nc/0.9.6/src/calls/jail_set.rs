/// Creates a new jail, or modifies an existing one, and optionally
/// locks the current process in it.
pub unsafe fn jail_set(iov: &mut [iovec_t], flags: i32) -> Result<i32, Errno> {
    let iov_ptr = iov.as_mut_ptr() as usize;
    let iov_len = iov.len();
    let flags = flags as usize;
    syscall3(SYS_JAIL_SET, iov_ptr, iov_len, flags).map(|val| val as i32)
}
