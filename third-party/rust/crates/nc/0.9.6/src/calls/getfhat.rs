/// Get file handle.
pub unsafe fn getfhat<P: AsRef<Path>>(
    fd: i32,
    path: P,
    fh: &mut fhandle_t,
    flag: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let fh_ptr = fh as *mut fhandle_t as usize;
    let flag = flag as usize;
    syscall4(SYS_GETFHAT, fd, path_ptr, fh_ptr, flag).map(drop)
}
