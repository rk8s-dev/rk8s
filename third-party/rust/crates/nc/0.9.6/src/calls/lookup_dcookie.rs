/// Return a directory entry's path.
pub unsafe fn lookup_dcookie(cookie: u64, buf: &mut [u8]) -> Result<ssize_t, Errno> {
    let cookie = cookie as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS_LOOKUP_DCOOKIE, cookie, buf_ptr, buf_len).map(|ret| ret as ssize_t)
}
