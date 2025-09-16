/// Get file handle.
pub unsafe fn getfh<P: AsRef<Path>>(path: P, fh: &mut fhandle_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let fh_ptr = fh as *mut fhandle_t as usize;
    syscall2(SYS_GETFH, path_ptr, fh_ptr).map(drop)
}
