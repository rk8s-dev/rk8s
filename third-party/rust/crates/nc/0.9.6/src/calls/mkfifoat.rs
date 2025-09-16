/// Create a fifo file.
pub unsafe fn mkfifoat<P: AsRef<Path>>(dfd: i32, path: P, mode: mode_t) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let mode = mode as usize;
    syscall3(SYS_MKFIFOAT, dfd, path_ptr, mode).map(drop)
}
