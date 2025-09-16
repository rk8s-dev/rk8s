pub unsafe fn __realpathat<P: AsRef<Path>>(
    fd: i32,
    path: P,
    buf: &mut [u8],
    flags: i32,
) -> Result<size_t, Errno> {
    let fd = fd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    let flags = flags as usize;
    syscall5(SYS___REALPATHAT, fd, path_ptr, buf_ptr, buf_len, flags)
}
