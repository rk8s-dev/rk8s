/// Get file handle, without following symbolic link.
pub unsafe fn lgetfh<P: AsRef<Path>>(path: P, fh: &mut fhandle_t) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let fh_ptr = fh as *mut fhandle_t as usize;
    syscall2(SYS_LGETFH, path_ptr, fh_ptr).map(drop)
}
