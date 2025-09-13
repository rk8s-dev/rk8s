/// Create a special or ordinary file.
pub unsafe fn __mknod50<P: AsRef<Path>>(
    filename: P,
    mode: mode_t,
    dev: dev_t,
) -> Result<(), Errno> {
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let mode = mode as usize;
    let dev = dev as usize;
    syscall3(SYS___MKNOD50, filename_ptr, mode, dev).map(drop)
}
