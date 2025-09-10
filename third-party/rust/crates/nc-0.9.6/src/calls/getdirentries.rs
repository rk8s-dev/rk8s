/// Get directory entries in a file system independent format
pub unsafe fn getdirentries(fd: i32, buf: &mut [c_char], off: off_t) -> Result<ssize_t, Errno> {
    let fd = fd as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    let off = off as usize;
    syscall4(SYS_GETDIRENTRIES, fd, buf_ptr, buf_len, off).map(|ret| ret as ssize_t)
}
