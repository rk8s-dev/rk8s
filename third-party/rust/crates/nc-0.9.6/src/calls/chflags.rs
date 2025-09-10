/// Set file flags.
pub unsafe fn chflags<P: AsRef<Path>>(path: P, flags: fflags_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    syscall2(SYS_CHFLAGS, path_ptr, flags).map(drop)
}
