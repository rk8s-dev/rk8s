/// Create a fifo file.
pub unsafe fn mkfifo<P: AsRef<Path>>(path: P, mode: mode_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let mode = mode as usize;
    syscall2(SYS_MKFIFO, path_ptr, mode).map(drop)
}
