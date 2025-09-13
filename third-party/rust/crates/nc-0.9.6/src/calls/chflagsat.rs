/// Set file flags.
pub unsafe fn chflagsat<P: AsRef<Path>>(
    fd: i32,
    path: P,
    flags: fflags_t,
    atflag: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    let atflag = atflag as usize;
    syscall4(SYS_CHFLAGSAT, fd, path_ptr, flags, atflag).map(drop)
}
